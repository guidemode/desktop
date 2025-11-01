use crate::config::load_provider_config;
use crate::events::{EventBus, SessionEventPayload};
use crate::logging::{log_error, log_info, log_warn};
use crate::providers::common::{
    extract_session_id_from_filename, get_file_size, has_extension, should_skip_file,
    SessionStateManager, WatcherStatus, EVENT_TIMEOUT, FILE_WATCH_POLL_INTERVAL,
    MIN_SIZE_CHANGE_BYTES,
};
use crate::providers::gemini::converter::convert_session_to_canonical;
use crate::providers::gemini_parser::GeminiSession;
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use shellexpand::tilde;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

const PROVIDER_ID: &str = "gemini-code";

#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub project_hash: String, // Project identified by hash
    pub session_id: String,
}

#[derive(Debug)]
pub struct GeminiWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: thread::JoinHandle<()>,
    upload_queue: Arc<UploadQueue>,
    is_running: Arc<Mutex<bool>>,
}

impl GeminiWatcher {
    pub fn new(
        project_hashes: Vec<String>, // Project hashes to watch
        upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Err(e) = log_info(PROVIDER_ID, "🔍 Starting Gemini Code file monitoring") {
            eprintln!("Logging error: {}", e);
        }

        // Load provider config to get home directory
        let config = load_provider_config(PROVIDER_ID)
            .map_err(|e| format!("Failed to load provider config: {}", e))?;

        if !config.enabled {
            return Err("Gemini Code provider is not enabled".into());
        }

        let home_directory = config.home_directory;
        let expanded_home = tilde(&home_directory);
        let base_path = Path::new(expanded_home.as_ref());

        if !base_path.exists() {
            return Err(format!(
                "Gemini Code home directory does not exist: {}",
                base_path.display()
            )
            .into());
        }

        let tmp_path = base_path.join("tmp");
        if !tmp_path.exists() {
            return Err(format!(
                "Gemini Code tmp directory does not exist: {}",
                tmp_path.display()
            )
            .into());
        }

        // Use the provided project hashes directly
        let projects_to_watch = project_hashes;

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📁 Monitoring {} Gemini Code projects",
                projects_to_watch.len()
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }

        // Create file system event channel
        let (tx, rx) = mpsc::channel();

        // Create the file watcher
        let mut watcher = RecommendedWatcher::new(
            tx,
            Config::default().with_poll_interval(FILE_WATCH_POLL_INTERVAL),
        )?;

        // Watch each project's chats directory
        for project_hash in &projects_to_watch {
            let chats_path = tmp_path.join(project_hash).join("chats");
            if chats_path.exists() && chats_path.is_dir() {
                watcher.watch(&chats_path, RecursiveMode::NonRecursive)?;
                if let Err(e) = log_info(
                    PROVIDER_ID,
                    &format!("📂 Watching Gemini project: {}", &project_hash[..8]),
                ) {
                    eprintln!("Logging error: {}", e);
                }
            } else if let Err(e) = log_warn(
                PROVIDER_ID,
                &format!(
                    "⚠ Project chats directory not found: {}",
                    chats_path.display()
                ),
            ) {
                eprintln!("Logging error: {}", e);
            }
        }

        let is_running = Arc::new(Mutex::new(true));
        let is_running_clone = Arc::clone(&is_running);
        let upload_queue_clone = Arc::clone(&upload_queue);
        let tmp_path_clone = tmp_path.clone();
        let event_bus_clone = event_bus.clone();

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(
                rx,
                tmp_path_clone,
                upload_queue_clone,
                event_bus_clone,
                is_running_clone,
            );
        });

        Ok(GeminiWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
        })
    }

    #[allow(dead_code)]
    fn discover_all_projects(
        tmp_path: &Path,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let entries = std::fs::read_dir(tmp_path)
            .map_err(|e| format!("Failed to read tmp directory: {}", e))?;

        let mut projects = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();

            // Skip 'bin' directory
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == "bin" {
                    continue;
                }
            }

            if path.is_dir() {
                // Check if chats directory exists
                let chats_path = path.join("chats");
                if chats_path.exists() && chats_path.is_dir() {
                    if let Some(hash) = path.file_name().and_then(|n| n.to_str()) {
                        projects.push(hash.to_string());
                    }
                }
            }
        }

        Ok(projects)
    }

    fn file_event_processor(
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
        tmp_path: PathBuf,
        _upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
        is_running: Arc<Mutex<bool>>,
    ) {
        let mut session_states = SessionStateManager::new();

        loop {
            // Check if we should continue running
            {
                if let Ok(running) = is_running.lock() {
                    if !*running {
                        break;
                    }
                }
            }

            // Process file system events with timeout
            match rx.recv_timeout(EVENT_TIMEOUT) {
                Ok(Ok(event)) => {
                    if let Some(file_event) = Self::process_file_event(&event, &tmp_path) {
                        // Convert Gemini JSON to canonical JSONL and cache it
                        let canonical_path = match Self::convert_to_canonical_file(
                            &file_event.path,
                            &file_event.session_id,
                        ) {
                            Ok(path) => path,
                            Err(e) => {
                                if let Err(log_err) = log_error(
                                    PROVIDER_ID,
                                    &format!("Failed to convert Gemini session {} to canonical format: {}", file_event.session_id, e),
                                ) {
                                    eprintln!("Logging error: {}", log_err);
                                }
                                continue; // Skip this event - don't crash on conversion errors
                            }
                        };

                        // Get file size of canonical JSONL
                        let canonical_size = get_file_size(&canonical_path).unwrap_or(0);

                        // Check if this is a new session (before get_or_create)
                        let is_new_session = !session_states.contains(&file_event.session_id);

                        // Get or create session state
                        let state =
                            session_states.get_or_create(&file_event.session_id, canonical_size);
                        let should_log =
                            state.should_log(canonical_size, MIN_SIZE_CHANGE_BYTES, is_new_session);

                        // Extract real project name from canonical JSONL (CWD -> project name)
                        // Fallback to shortened hash if CWD extraction fails
                        let project_name = Self::extract_project_name_from_jsonl(&canonical_path)
                            .unwrap_or_else(|| format!("gemini-{}", &file_event.project_hash[..8]));

                        // Publish SessionChanged event with CANONICAL path
                        // DatabaseEventHandler will call db_helpers which does smart insert-or-update
                        let payload = SessionEventPayload::SessionChanged {
                            session_id: file_event.session_id.clone(),
                            project_name, // Real project name extracted from CWD
                            file_path: canonical_path.clone(), // Use canonical path (not original JSON)
                            file_size: canonical_size,         // Use canonical file size
                        };

                        if let Err(e) = event_bus.publish(PROVIDER_ID, payload) {
                            if let Err(log_err) = log_error(
                                PROVIDER_ID,
                                &format!("Failed to publish session event: {}", e),
                            ) {
                                eprintln!("Logging error: {}", log_err);
                            }
                        }

                        // Update session state immediately to prevent duplicate events
                        state.update(canonical_size);

                        // Mark session as seen so it's not treated as new again
                        if is_new_session {
                            state.mark_as_seen();
                        }

                        if should_log {
                            if is_new_session {
                                let log_message = format!(
                                    "🆕 New Gemini session detected: {}",
                                    file_event.session_id
                                );
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Log session updates at info level
                                let log_message = format!(
                                    "📝 Gemini session changed: {} (size: {} bytes)",
                                    file_event.session_id, canonical_size
                                );
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            }
                        }
                    }
                }
                Ok(Err(error)) => {
                    if let Err(e) =
                        log_error(PROVIDER_ID, &format!("File watcher error: {:?}", error))
                    {
                        eprintln!("Logging error: {}", e);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout is normal, continue
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Err(e) = log_error(PROVIDER_ID, "File watcher channel disconnected") {
                        eprintln!("Logging error: {}", e);
                    }
                    break;
                }
            }
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Gemini Code file monitoring stopped") {
            eprintln!("Logging error: {}", e);
        }
    }

    fn process_file_event(event: &Event, tmp_path: &Path) -> Option<FileChangeEvent> {
        // Only process write events for session JSON files
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    // Skip hidden files
                    if should_skip_file(path) {
                        continue;
                    }

                    // Check if it's a session JSON file
                    if !has_extension(path, "json") {
                        continue;
                    }

                    // Check if filename starts with "session-"
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if !filename.starts_with("session-") {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    // Extract project hash from path
                    if let Some(project_hash) = Self::extract_project_hash(path, tmp_path) {
                        // Get session ID
                        let session_id = extract_session_id_from_filename(path);

                        return Some(FileChangeEvent {
                            path: path.clone(),
                            project_hash,
                            session_id: session_id.clone(),
                        });
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn extract_project_hash(file_path: &Path, tmp_path: &Path) -> Option<String> {
        // Path structure: ~/.gemini/tmp/{hash}/chats/session-{timestamp}-{id}.json
        // We need to extract {hash}

        if let Ok(relative_path) = file_path.strip_prefix(tmp_path) {
            // First component should be the hash
            if let Some(first_component) = relative_path.components().next() {
                if let Some(hash) = first_component.as_os_str().to_str() {
                    return Some(hash.to_string());
                }
            }
        }
        None
    }

    /// Convert Gemini JSON file to canonical JSONL and cache it
    /// Returns the path to the cached canonical JSONL file
    fn convert_to_canonical_file(
        json_file_path: &Path,
        session_id: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        // Get canonical cache directory
        let cache_base = dirs::home_dir()
            .ok_or("Failed to get home directory")?
            .join(".guideai")
            .join("cache")
            .join("canonical")
            .join(PROVIDER_ID);

        // Create cache directory if it doesn't exist
        fs::create_dir_all(&cache_base)?;

        // Output file path
        let cache_path = cache_base.join(format!("{}.jsonl", session_id));

        // Read the original Gemini JSON file
        let content = fs::read_to_string(json_file_path)?;

        // Parse the Gemini session
        let session = GeminiSession::from_json(&content)?;

        // Try to infer CWD from message content
        let cwd = Self::infer_cwd_from_session(&session);

        // Convert to canonical format
        let canonical_messages = convert_session_to_canonical(&session, cwd)?;

        // Serialize each message to JSONL
        let mut canonical_lines = Vec::new();
        for (line_num, msg) in canonical_messages.iter().enumerate() {
            match serde_json::to_string(msg) {
                Ok(line) => canonical_lines.push(line),
                Err(e) => {
                    if let Err(log_err) = log_error(
                        PROVIDER_ID,
                        &format!(
                            "Failed to serialize canonical message {} for session {}: {}",
                            line_num, session_id, e
                        ),
                    ) {
                        eprintln!("Logging error: {}", log_err);
                    }
                    // Continue processing other messages
                }
            }
        }

        // Write to canonical cache
        fs::write(&cache_path, canonical_lines.join("\n"))?;

        Ok(cache_path)
    }

    /// Infer working directory from Gemini session messages
    /// Uses the shared CWD extraction function from gemini_utils.rs
    fn infer_cwd_from_session(session: &GeminiSession) -> Option<String> {
        use super::gemini_utils::infer_cwd_from_session as shared_infer_cwd;
        shared_infer_cwd(session, &session.project_hash)
    }

    /// Extract project name from JSONL file by reading CWD field
    /// Returns the last path component of the CWD (e.g., "/Users/cliftonc/work/guideai" -> "guideai")
    fn extract_project_name_from_jsonl(jsonl_path: &PathBuf) -> Option<String> {
        // Read first few lines to find CWD
        let content = fs::read_to_string(jsonl_path).ok()?;
        let lines: Vec<&str> = content.lines().take(50).collect();

        // Find first line with a CWD field
        for line in lines {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(cwd) = entry.get("cwd").and_then(|v| v.as_str()) {
                    // Extract project name from CWD path
                    return Path::new(cwd)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string());
                }
            }
        }

        None
    }

    pub fn stop(&self) {
        if let Ok(mut running) = self.is_running.lock() {
            *running = false;
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping Gemini Code file monitoring") {
            eprintln!("Logging error: {}", e);
        }
    }

    pub fn get_status(&self) -> WatcherStatus {
        let is_running = if let Ok(running) = self.is_running.lock() {
            *running
        } else {
            false
        };

        let upload_status = self.upload_queue.get_status();

        WatcherStatus {
            is_running,
            pending_uploads: upload_status.pending,
            processing_uploads: upload_status.processing,
            failed_uploads: upload_status.failed,
        }
    }
}

// Type alias for backward compatibility
pub type GeminiWatcherStatus = WatcherStatus;

impl Drop for GeminiWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_session_id_from_filename() {
        let path = Path::new("/path/to/session-2025-10-11T07-44-ae9730b6.json");
        let session_id = extract_session_id_from_filename(path);
        assert_eq!(session_id, "session-2025-10-11T07-44-ae9730b6");
    }

    #[test]
    fn test_extract_project_hash() {
        let tmp_path = Path::new("/home/user/.gemini/tmp");
        let file_path = Path::new(
            "/home/user/.gemini/tmp/7e95bdea1c91b994ca74439a92c90b82767abc9c0b8566e20ab60b2a797fc332/chats/session-123.json",
        );

        let hash = GeminiWatcher::extract_project_hash(file_path, tmp_path);
        assert_eq!(
            hash,
            Some("7e95bdea1c91b994ca74439a92c90b82767abc9c0b8566e20ab60b2a797fc332".to_string())
        );
    }

    // Tests for CWD extraction helpers have been moved to gemini.rs
    // (extract_candidate_paths_from_content, find_matching_path, verify_hash are now shared functions)
}
