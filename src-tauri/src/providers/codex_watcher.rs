use crate::config::load_provider_config;
use crate::logging::{log_debug, log_error, log_info, log_warn};
use crate::upload_queue::UploadQueue;
use serde::{Deserialize, Serialize};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use shellexpand::tilde;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ID: &str = "codex";
const DEBOUNCE_DURATION: Duration = Duration::from_secs(30); // 30 seconds for active sessions
const QUICK_DEBOUNCE_DURATION: Duration = Duration::from_secs(5); // 5 seconds for new files

// In dev mode, upload much faster for testing
#[cfg(debug_assertions)]
const ACTIVE_SESSION_TIMEOUT: Duration = Duration::from_secs(5); // Mark session inactive after 5s (dev mode)

#[cfg(not(debug_assertions))]
const ACTIVE_SESSION_TIMEOUT: Duration = Duration::from_secs(60); // Mark session inactive after 60s (production)

// Minimum time between re-uploads to prevent spam
#[cfg(debug_assertions)]
const RE_UPLOAD_COOLDOWN: Duration = Duration::from_secs(30); // 30 seconds in dev mode

#[cfg(not(debug_assertions))]
const RE_UPLOAD_COOLDOWN: Duration = Duration::from_secs(300); // 5 minutes in production

const MIN_SIZE_CHANGE_BYTES: u64 = 1024; // Minimum 1KB change to trigger upload

#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub project_name: String,
    pub last_modified: Instant,
    pub file_size: u64,
    pub session_id: String,
    pub is_new_session: bool,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub last_modified: Instant,
    pub last_size: u64,
    pub is_active: bool,
    pub upload_pending: bool,
    pub last_uploaded_time: Option<Instant>,
    pub last_uploaded_size: u64,
}

#[derive(Debug)]
pub struct CodexWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: thread::JoinHandle<()>,
    upload_queue: Arc<UploadQueue>,
    is_running: Arc<Mutex<bool>>,
}

impl CodexWatcher {
    pub fn new(
        _projects: Vec<String>,
        upload_queue: Arc<UploadQueue>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Err(e) = log_info(PROVIDER_ID, "🔍 Starting Codex file monitoring") {
            eprintln!("Logging error: {}", e);
        }

        // Load provider config to get home directory
        let config = load_provider_config(PROVIDER_ID)
            .map_err(|e| format!("Failed to load provider config: {}", e))?;

        if !config.enabled {
            return Err("Codex provider is not enabled".into());
        }

        let home_directory = config.home_directory;
        let expanded_home = tilde(&home_directory);
        let base_path = Path::new(expanded_home.as_ref());

        if !base_path.exists() {
            return Err(format!("Codex home directory does not exist: {}", base_path.display()).into());
        }

        let sessions_path = base_path.join("sessions");
        if !sessions_path.exists() {
            return Err(format!("Codex sessions directory does not exist: {}", sessions_path.display()).into());
        }

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!("📁 Monitoring Codex sessions directory: {}", sessions_path.display()),
        ) {
            eprintln!("Logging error: {}", e);
        }

        // Create file system event channel
        let (tx, rx) = mpsc::channel();

        // Create the file watcher
        let mut watcher = RecommendedWatcher::new(
            tx,
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        // Watch the entire sessions directory recursively (includes YYYY/MM/DD subdirs)
        watcher.watch(&sessions_path, RecursiveMode::Recursive)?;
        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!("📂 Watching Codex sessions: {}", sessions_path.display()),
        ) {
            eprintln!("Logging error: {}", e);
        }

        let is_running = Arc::new(Mutex::new(true));
        let is_running_clone = Arc::clone(&is_running);
        let upload_queue_clone = Arc::clone(&upload_queue);
        let sessions_path_clone = sessions_path.clone();

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(
                rx,
                sessions_path_clone,
                upload_queue_clone,
                is_running_clone,
            );
        });

        Ok(CodexWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
        })
    }

    fn file_event_processor(
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
        sessions_path: PathBuf,
        upload_queue: Arc<UploadQueue>,
        is_running: Arc<Mutex<bool>>,
    ) {
        let mut pending_files: HashMap<PathBuf, FileChangeEvent> = HashMap::new();
        let mut session_states: HashMap<String, SessionState> = HashMap::new();

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
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(event)) => {
                    if let Some(file_event) = Self::process_file_event(&event, &sessions_path, &session_states) {
                        // Check if this is a new session or significant change (before updating state)
                        let should_log = Self::should_log_event(&file_event, &session_states);

                        // Update session state immediately to prevent duplicate events
                        Self::update_session_state(&mut session_states, &file_event);

                        if should_log {
                            if file_event.is_new_session {
                                let log_message = format!("🆕 New Codex session detected: {} → Queuing for upload", file_event.session_id);
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Use debug level for routine session activity
                                let log_message = format!("📝 Codex session active: {} (size: {} bytes)", file_event.session_id, file_event.file_size);
                                if let Err(e) = log_debug(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            }
                        }

                        pending_files.insert(file_event.path.clone(), file_event);
                    }
                }
                Ok(Err(error)) => {
                    if let Err(e) = log_error(PROVIDER_ID, &format!("Codex file watcher error: {:?}", error)) {
                        eprintln!("Logging error: {}", e);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout is normal, continue to check pending files
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Err(e) = log_error(PROVIDER_ID, "Codex file watcher channel disconnected") {
                        eprintln!("Logging error: {}", e);
                    }
                    break;
                }
            }

            // Check for files ready to upload (smart debouncing)
            let now = Instant::now();
            let mut ready_files = Vec::new();

            for (path, file_event) in &pending_files {
                let debounce_duration = if file_event.is_new_session {
                    QUICK_DEBOUNCE_DURATION
                } else {
                    DEBOUNCE_DURATION
                };

                let should_upload = if file_event.is_new_session {
                    // Upload new sessions more quickly
                    now.duration_since(file_event.last_modified) >= debounce_duration
                } else {
                    // For existing sessions, check if session has become inactive
                    Self::should_upload_session(&file_event.session_id, &session_states, now)
                };

                if should_upload {
                    ready_files.push(path.clone());
                }
            }

            // Process ready files
            for path in ready_files {
                if let Some(file_event) = pending_files.remove(&path) {
                    // Mark session as uploaded and track upload metadata
                    if let Some(session_state) = session_states.get_mut(&file_event.session_id) {
                        session_state.upload_pending = true;
                        session_state.last_uploaded_time = Some(now);
                        session_state.last_uploaded_size = file_event.file_size;
                    }

                    if let Err(e) = upload_queue.add_item(
                        PROVIDER_ID,
                        &file_event.project_name,
                        file_event.path.clone(),
                    ) {
                        if let Err(log_err) = log_error(
                            PROVIDER_ID,
                            &format!("✗ Failed to queue Codex session {} for upload: {}", file_event.session_id, e),
                        ) {
                            eprintln!("Logging error: {}", log_err);
                        }
                    } else {
                        if let Err(e) = log_info(
                            PROVIDER_ID,
                            &format!("📤 Codex session {} queued for upload ({})", file_event.session_id, file_event.path.file_name().unwrap_or_default().to_string_lossy()),
                        ) {
                            eprintln!("Logging error: {}", e);
                        }
                    }
                }
            }

            // Clean up old session states
            Self::cleanup_old_sessions(&mut session_states, now);
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Codex file monitoring stopped") {
            eprintln!("Logging error: {}", e);
        }
    }

    fn process_file_event(
        event: &Event,
        sessions_path: &Path,
        session_states: &HashMap<String, SessionState>,
    ) -> Option<FileChangeEvent> {
        // Only process write events for .jsonl files
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    // Skip hidden files (starting with .)
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if file_name.starts_with('.') {
                            continue;
                        }
                    }

                    // Check if it's a .jsonl file
                    if let Some(extension) = path.extension() {
                        if extension != "jsonl" {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    // Ensure it's within the sessions directory
                    if !path.starts_with(sessions_path) {
                        continue;
                    }

                    // Extract project name and session ID from path
                    if let Some((project_name, session_id)) = Self::extract_session_info(path) {
                        // Get file size
                        let file_size = Self::get_file_size(path).unwrap_or(0);

                        return Some(FileChangeEvent {
                            path: path.clone(),
                            project_name,
                            last_modified: Instant::now(),
                            file_size,
                            session_id: session_id.clone(),
                            is_new_session: Self::is_new_session(&session_id, path, session_states),
                        });
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn extract_session_info(file_path: &Path) -> Option<(String, String)> {
        // Read first line to get session metadata
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(file_path).ok()?;
        let reader = BufReader::new(file);
        let first_line = reader.lines().next()?.ok()?;

        #[derive(Deserialize)]
        struct SessionMeta {
            #[serde(rename = "type")]
            entry_type: Option<String>,
            payload: Option<SessionPayload>,
        }

        #[derive(Deserialize)]
        struct SessionPayload {
            id: Option<String>,
            cwd: Option<String>,
        }

        let meta: SessionMeta = serde_json::from_str(&first_line).ok()?;

        // Check if this is a session_meta entry
        if meta.entry_type.as_deref() == Some("session_meta") {
            if let Some(payload) = meta.payload {
                let session_id = payload.id?;
                let cwd = payload.cwd?;

                // Extract project name from cwd path
                let project_name = Path::new(&cwd)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                return Some((project_name, session_id));
            }
        }

        None
    }

    fn get_file_size(path: &Path) -> Result<u64, std::io::Error> {
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
    }

    fn is_new_session(session_id: &str, path: &Path, session_states: &HashMap<String, SessionState>) -> bool {
        // First check if we've already seen this session
        if session_states.contains_key(session_id) {
            return false; // Already tracking this session
        }

        // Check if this session file is new by looking at file size
        // A new session typically starts with a small file size
        if let Ok(metadata) = std::fs::metadata(path) {
            let file_size = metadata.len();
            // Consider it a new session if file is small (less than 5KB)
            // This indicates it's just starting
            file_size < 5120
        } else {
            true // If we can't read metadata, assume it's new
        }
    }

    fn should_log_event(file_event: &FileChangeEvent, session_states: &HashMap<String, SessionState>) -> bool {
        match session_states.get(&file_event.session_id) {
            Some(existing_state) => {
                // Only log if significant size change or new session
                file_event.is_new_session ||
                file_event.file_size.saturating_sub(existing_state.last_size) >= MIN_SIZE_CHANGE_BYTES
            },
            None => {
                // Only log if this is actually a new session (small file size)
                // This prevents duplicate logging for the same session when multiple
                // file events occur before the session state is updated
                file_event.is_new_session
            }
        }
    }

    fn update_session_state(session_states: &mut HashMap<String, SessionState>, file_event: &FileChangeEvent) {
        match session_states.get_mut(&file_event.session_id) {
            Some(existing_state) => {
                // Update existing session state
                existing_state.last_modified = file_event.last_modified;
                existing_state.last_size = file_event.file_size;
                existing_state.is_active = true;

                // Smart re-upload logic: clear upload_pending if conditions met
                if existing_state.upload_pending {
                    let should_allow_reupload = if let Some(last_uploaded_time) = existing_state.last_uploaded_time {
                        // Check if cooldown has elapsed OR size changed significantly
                        let cooldown_elapsed = file_event.last_modified.duration_since(last_uploaded_time) >= RE_UPLOAD_COOLDOWN;
                        let size_changed_significantly = file_event.file_size.saturating_sub(existing_state.last_uploaded_size) >= MIN_SIZE_CHANGE_BYTES;

                        cooldown_elapsed || size_changed_significantly
                    } else {
                        // No last upload time recorded, allow re-upload
                        true
                    };

                    if should_allow_reupload {
                        existing_state.upload_pending = false;
                    }
                }
            },
            None => {
                // Create new session state
                let session_state = SessionState {
                    last_modified: file_event.last_modified,
                    last_size: file_event.file_size,
                    is_active: true,
                    upload_pending: false,
                    last_uploaded_time: None,
                    last_uploaded_size: 0,
                };
                session_states.insert(file_event.session_id.clone(), session_state);
            }
        }
    }

    fn should_upload_session(session_id: &str, session_states: &HashMap<String, SessionState>, now: Instant) -> bool {
        if let Some(session_state) = session_states.get(session_id) {
            // Upload if session has been inactive for the timeout duration and upload not already pending
            now.duration_since(session_state.last_modified) >= ACTIVE_SESSION_TIMEOUT && !session_state.upload_pending
        } else {
            false
        }
    }

    fn cleanup_old_sessions(session_states: &mut HashMap<String, SessionState>, now: Instant) {
        // Remove sessions that are older than 5 minutes and have been uploaded
        let cleanup_threshold = Duration::from_secs(300); // 5 minutes

        session_states.retain(|_, state| {
            now.duration_since(state.last_modified) < cleanup_threshold || !state.upload_pending
        });
    }

    pub fn stop(&self) {
        if let Ok(mut running) = self.is_running.lock() {
            *running = false;
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping Codex file monitoring") {
            eprintln!("Logging error: {}", e);
        }
    }

    pub fn get_status(&self) -> CodexWatcherStatus {
        let is_running = if let Ok(running) = self.is_running.lock() {
            *running
        } else {
            false
        };

        let upload_status = self.upload_queue.get_status();

        CodexWatcherStatus {
            is_running,
            pending_uploads: upload_status.pending,
            processing_uploads: upload_status.processing,
            failed_uploads: upload_status.failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexWatcherStatus {
    pub is_running: bool,
    pub pending_uploads: usize,
    pub processing_uploads: usize,
    pub failed_uploads: usize,
}

impl Drop for CodexWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_new_session() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test-session.jsonl");
        let session_id = "test-session";
        let mut session_states = HashMap::new();

        // Create a small file - should be considered new
        fs::write(&file_path, r#"{"timestamp":"2025-01-01T10:00:00.000Z","type":"session_meta"}"#).unwrap();
        assert!(CodexWatcher::is_new_session(session_id, &file_path, &session_states));

        // Add session to states - should not be considered new even if file is small
        session_states.insert(session_id.to_string(), SessionState {
            last_modified: Instant::now(),
            last_size: 100,
            is_active: true,
            upload_pending: false,
            last_uploaded_time: None,
            last_uploaded_size: 0,
        });
        assert!(!CodexWatcher::is_new_session(session_id, &file_path, &session_states));
    }
}