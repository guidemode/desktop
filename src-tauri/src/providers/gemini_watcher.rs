use crate::config::load_provider_config;
use crate::events::{EventBus, SessionEventPayload};
use crate::logging::{log_error, log_info, log_warn};
use crate::providers::common::{
    extract_session_id_from_filename, get_file_size, has_extension, should_skip_file,
    SessionStateManager, WatcherStatus, EVENT_TIMEOUT, FILE_WATCH_POLL_INTERVAL,
    MIN_SIZE_CHANGE_BYTES,
};
use crate::providers::gemini_parser::GeminiSession;
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use shellexpand::tilde;
use std::fs;
use std::io::Write;
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

        // Determine which projects to watch
        let projects_to_watch = if config.project_selection == "ALL" {
            // Watch all available project hashes
            Self::discover_all_projects(&tmp_path)?
        } else {
            // Watch only selected projects
            let selected_projects: Vec<String> = config
                .selected_projects
                .into_iter()
                .filter(|project| project_hashes.contains(project))
                .collect();

            if selected_projects.is_empty() {
                return Err("No valid projects selected for watching".into());
            }

            selected_projects
        };

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📁 Monitoring {} Gemini Code projects (hashes)",
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
            } else {
                if let Err(e) = log_warn(
                    PROVIDER_ID,
                    &format!("⚠ Project chats directory not found: {}", chats_path.display()),
                ) {
                    eprintln!("Logging error: {}", e);
                }
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
                    if let Some(file_event) =
                        Self::process_file_event(&event, &tmp_path)
                    {
                        // Convert Gemini JSON to JSONL and cache it
                        let jsonl_path = match Self::convert_to_jsonl_and_cache(
                            &file_event.path,
                            &file_event.session_id,
                        ) {
                            Ok(path) => path,
                            Err(e) => {
                                if let Err(log_err) = log_error(
                                    PROVIDER_ID,
                                    &format!(
                                        "Failed to convert Gemini JSON to JSONL: {}",
                                        e
                                    ),
                                ) {
                                    eprintln!("Logging error: {}", log_err);
                                }
                                continue; // Skip this event
                            }
                        };

                        // Get file size of JSONL
                        let jsonl_size = get_file_size(&jsonl_path).unwrap_or(0);

                        // Check if this is a new session (before get_or_create)
                        let is_new_session = !session_states.contains(&file_event.session_id);

                        // Get or create session state
                        let state = session_states.get_or_create(
                            &file_event.session_id,
                            jsonl_size,
                        );
                        let should_log = state.should_log(
                            jsonl_size,
                            MIN_SIZE_CHANGE_BYTES,
                            is_new_session,
                        );

                        // Publish SessionChanged event to event bus
                        // DatabaseEventHandler will call db_helpers which does smart insert-or-update
                        // Note: project_hash is used as temporary project_name here
                        let payload = SessionEventPayload::SessionChanged {
                            session_id: file_event.session_id.clone(),
                            project_name: file_event.project_hash.clone(), // Temporary - db_helpers will extract real name
                            file_path: jsonl_path.clone(), // Use JSONL path instead of original JSON
                            file_size: jsonl_size,  // Use JSONL file size
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
                        state.update(jsonl_size);

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
                                    file_event.session_id, jsonl_size
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

    fn process_file_event(
        event: &Event,
        tmp_path: &Path,
    ) -> Option<FileChangeEvent> {
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


    /// Convert Gemini JSON file to JSONL and cache it
    /// Returns the path to the cached JSONL file
    fn convert_to_jsonl_and_cache(
        json_file_path: &Path,
        session_id: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        // Read the original Gemini JSON file
        let content = fs::read_to_string(json_file_path)?;

        // Parse the Gemini session
        let session = GeminiSession::from_json(&content)?;

        // Try to infer CWD from message content
        let cwd = Self::infer_cwd_from_session(&session);

        // Convert to JSONL
        let jsonl_content = session.to_jsonl(cwd.as_deref())?;

        // Create cache directory: ~/.guideai/cache/gemini-code/
        let cache_dir = dirs::home_dir()
            .ok_or("Could not determine home directory")?
            .join(".guideai")
            .join("cache")
            .join(PROVIDER_ID);

        fs::create_dir_all(&cache_dir)?;

        // Write JSONL to cache
        let jsonl_path = cache_dir.join(format!("{}.jsonl", session_id));
        let mut file = fs::File::create(&jsonl_path)?;
        file.write_all(jsonl_content.as_bytes())?;

        Ok(jsonl_path)
    }

    /// Infer working directory from Gemini session messages
    fn infer_cwd_from_session(session: &GeminiSession) -> Option<String> {
        // Check ALL messages (user and gemini), since file paths can appear in:
        // - User messages containing tool responses (e.g., "[Function Response: read_file]--- /Users/...")
        // - Gemini messages containing file references
        for message in &session.messages {
            // Get all candidate paths from this message
            let candidate_paths = Self::extract_candidate_paths_from_content(&message.content);

            // Try each candidate path, testing progressively shorter paths
            for base_path in candidate_paths {
                if let Some(matching_path) = Self::find_matching_path(&base_path, &session.project_hash) {
                    return Some(matching_path);
                }
            }
        }
        None
    }

    /// Extract all candidate file paths from message content
    fn extract_candidate_paths_from_content(content: &str) -> Vec<String> {
        let mut paths = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for line in lines {
            // Look for absolute paths (Unix and Windows)
            if line.contains("/Users/") || line.contains("/home/") || line.contains("C:\\") {
                // Prefer paths after '---' delimiter (common in tool output)
                let search_text = if let Some(delimiter_pos) = line.find("---") {
                    &line[delimiter_pos + 3..]
                } else {
                    line
                };

                // Extract all absolute paths from the line
                let parts: Vec<&str> = search_text.split_whitespace().collect();
                for part in parts {
                    // Unix paths
                    if part.starts_with('/') {
                        paths.push(part.to_string());
                    }
                    // Windows paths
                    else if part.len() > 3 && part.chars().nth(1) == Some(':') && part.chars().nth(2) == Some('\\') {
                        paths.push(part.to_string());
                    }
                }
            }
        }

        paths
    }

    /// Try progressively shorter paths until we find one matching the hash
    fn find_matching_path(full_path: &str, expected_hash: &str) -> Option<String> {
        use std::path::Path;

        let path_buf = Path::new(full_path);
        let mut current_path = path_buf;

        // Try the full path first, then progressively remove the last segment
        loop {
            if let Some(path_str) = current_path.to_str() {
                // Skip root and empty paths
                if !path_str.is_empty() && path_str != "/" && path_str != "\\" {
                    // Test if this path's hash matches
                    if Self::verify_hash(path_str, expected_hash) {
                        return Some(path_str.to_string());
                    }
                }
            }

            // Move up to parent directory
            match current_path.parent() {
                Some(parent) if parent != current_path => {
                    current_path = parent;
                }
                _ => break, // No more parents or reached root
            }
        }

        None
    }

    /// Verify that SHA256(workdir) == hash
    fn verify_hash(workdir: &str, expected_hash: &str) -> bool {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(workdir.as_bytes());
        let result = hasher.finalize();
        let computed_hash = hex::encode(result);
        computed_hash == expected_hash
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

    #[test]
    fn test_extract_candidate_paths_from_content() {
        let content = r#"
--- /Users/cliftonc/work/guideai/CLAUDE.md ---
Some content here
--- /Users/cliftonc/work/guideai/apps/desktop/CLAUDE.md ---
More content
"#;

        let paths = GeminiWatcher::extract_candidate_paths_from_content(content);
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&"/Users/cliftonc/work/guideai/CLAUDE.md".to_string()));
        assert!(paths.contains(&"/Users/cliftonc/work/guideai/apps/desktop/CLAUDE.md".to_string()));
    }

    #[test]
    fn test_extract_candidate_paths_no_delimiter() {
        let content = "Reading file /home/user/projects/myapp/src/main.rs";

        let paths = GeminiWatcher::extract_candidate_paths_from_content(content);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "/home/user/projects/myapp/src/main.rs");
    }

    #[test]
    fn test_extract_candidate_paths_multiple_per_line() {
        let content = "Comparing /Users/test/work/app/file1.txt and /Users/test/work/app/file2.txt";

        let paths = GeminiWatcher::extract_candidate_paths_from_content(content);
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn test_find_matching_path_exact_match() {
        // Hash for "/Users/cliftonc/work/guideai"
        let expected_hash = "7e95bdea1c91b994ca74439a92c90b82767abc9c0b8566e20ab60b2a797fc332";
        let full_path = "/Users/cliftonc/work/guideai/CLAUDE.md";

        let result = GeminiWatcher::find_matching_path(full_path, expected_hash);
        assert_eq!(result, Some("/Users/cliftonc/work/guideai".to_string()));
    }

    #[test]
    fn test_find_matching_path_nested_file() {
        // Hash for "/Users/cliftonc/work/guideai"
        let expected_hash = "7e95bdea1c91b994ca74439a92c90b82767abc9c0b8566e20ab60b2a797fc332";
        let full_path = "/Users/cliftonc/work/guideai/apps/desktop/src/main.rs";

        let result = GeminiWatcher::find_matching_path(full_path, expected_hash);
        assert_eq!(result, Some("/Users/cliftonc/work/guideai".to_string()));
    }

    #[test]
    fn test_find_matching_path_no_match() {
        // Random hash that won't match
        let expected_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let full_path = "/Users/test/project/file.txt";

        let result = GeminiWatcher::find_matching_path(full_path, expected_hash);
        assert_eq!(result, None);
    }

    #[test]
    fn test_verify_hash() {
        // Known hash for "/Users/cliftonc/work/guideai"
        let workdir = "/Users/cliftonc/work/guideai";
        let expected_hash = "7e95bdea1c91b994ca74439a92c90b82767abc9c0b8566e20ab60b2a797fc332";

        assert!(GeminiWatcher::verify_hash(workdir, expected_hash));
    }

    #[test]
    fn test_verify_hash_mismatch() {
        let workdir = "/Users/cliftonc/work/guideai";
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        assert!(!GeminiWatcher::verify_hash(workdir, wrong_hash));
    }
}
