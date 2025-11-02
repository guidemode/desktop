use crate::config::load_provider_config;
use crate::events::{EventBus, SessionEventPayload};
use crate::logging::{log_error, log_info};
use crate::providers::common::{
    extract_session_id_from_filename, get_file_size, has_extension, should_skip_file,
    SessionStateManager, WatcherStatus, EVENT_TIMEOUT, FILE_WATCH_POLL_INTERVAL,
    MIN_SIZE_CHANGE_BYTES,
};
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use shellexpand::tilde;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

const PROVIDER_ID: &str = "github-copilot";

#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub project_name: String,
    pub file_size: u64,
    pub session_id: String,
}

#[derive(Debug)]
pub struct CopilotWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: thread::JoinHandle<()>,
    upload_queue: Arc<UploadQueue>,
    is_running: Arc<Mutex<bool>>,
}

impl CopilotWatcher {
    pub fn new(
        _projects: Vec<String>,
        upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Err(e) = log_info(PROVIDER_ID, "🔍 Starting GitHub Copilot file monitoring") {
            eprintln!("Logging error: {}", e);
        }

        // Load provider config to get home directory
        let config = load_provider_config(PROVIDER_ID)
            .map_err(|e| format!("Failed to load provider config: {}", e))?;

        if !config.enabled {
            return Err("GitHub Copilot provider is not enabled".into());
        }

        let home_directory = config.home_directory;
        let expanded_home = tilde(&home_directory);
        let base_path = Path::new(expanded_home.as_ref());

        if !base_path.exists() {
            return Err(format!(
                "GitHub Copilot home directory does not exist: {}",
                base_path.display()
            )
            .into());
        }

        let session_dir = base_path.join("session-state");
        if !session_dir.exists() {
            return Err(format!(
                "GitHub Copilot session directory does not exist: {}",
                session_dir.display()
            )
            .into());
        }

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📁 Monitoring GitHub Copilot sessions: {}",
                session_dir.display()
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

        // Watch the session directory
        watcher.watch(&session_dir, RecursiveMode::NonRecursive)?;
        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📂 Watching GitHub Copilot sessions: {}",
                session_dir.display()
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }

        let is_running = Arc::new(Mutex::new(true));
        let is_running_clone = Arc::clone(&is_running);
        let upload_queue_clone = Arc::clone(&upload_queue);
        let event_bus_clone = event_bus.clone();

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(
                rx,
                session_dir,
                upload_queue_clone,
                event_bus_clone,
                is_running_clone,
            );
        });

        Ok(CopilotWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
        })
    }

    fn file_event_processor(
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
        session_dir: PathBuf,
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
                    if let Some(file_event) = Self::process_file_event(&event, &session_dir) {
                        // Check if this is a new session (before get_or_create)
                        let is_new_session = !session_states.contains(&file_event.session_id);

                        // Get or create session state
                        let state = session_states
                            .get_or_create(&file_event.session_id, file_event.file_size);
                        let should_log = state.should_log(
                            file_event.file_size,
                            MIN_SIZE_CHANGE_BYTES,
                            is_new_session,
                        );

                        // Publish SessionChanged event to event bus
                        let payload = SessionEventPayload::SessionChanged {
                            session_id: file_event.session_id.clone(),
                            project_name: file_event.project_name.clone(),
                            file_path: file_event.path.clone(),
                            file_size: file_event.file_size,
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
                        state.update(file_event.file_size);

                        // Mark session as seen so it's not treated as new again
                        if is_new_session {
                            state.mark_as_seen();
                        }

                        // Log events
                        if should_log {
                            if is_new_session {
                                if let Err(e) = log_info(
                                    PROVIDER_ID,
                                    &format!(
                                        "🆕 New Copilot session detected: {}",
                                        file_event.session_id
                                    ),
                                ) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Log session updates at info level
                                if let Err(e) = log_info(
                                    PROVIDER_ID,
                                    &format!(
                                        "📝 Copilot session changed: {} (size: {} bytes)",
                                        file_event.session_id, file_event.file_size
                                    ),
                                ) {
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
                    // Timeout is normal, continue to check pending files
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Err(e) = log_error(PROVIDER_ID, "File watcher channel disconnected") {
                        eprintln!("Logging error: {}", e);
                    }
                    break;
                }
            }
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 GitHub Copilot file monitoring stopped") {
            eprintln!("Logging error: {}", e);
        }
    }

    fn convert_to_canonical_file(
        copilot_file: &Path,
        session_id: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        use crate::providers::copilot_parser::CopilotParser;
        use std::fs;

        // Get canonical cache directory
        let cache_base = dirs::home_dir()
            .ok_or("Failed to get home directory")?
            .join(".guideai")
            .join("cache")
            .join("canonical")
            .join(PROVIDER_ID);

        // Create cache directory if it doesn't exist
        fs::create_dir_all(&cache_base)?;

        // Create canonical cache file path (session_id.jsonl)
        let cache_path = cache_base.join(format!("{}.jsonl", session_id));

        // Parse and convert to canonical format using the parser
        let storage_path = copilot_file
            .parent()
            .and_then(|p| p.parent())
            .ok_or("Invalid file path")?;

        let parser = CopilotParser::new(storage_path.to_path_buf());
        let parsed = parser.parse_session(copilot_file)?;

        // Write canonical JSONL to cache
        fs::write(&cache_path, parsed.jsonl_content)?;

        Ok(cache_path)
    }

    fn process_file_event(event: &Event, session_dir: &Path) -> Option<FileChangeEvent> {
        // Only process write events for .jsonl files
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    // Check if it's in the session directory
                    if !path.starts_with(session_dir) {
                        continue;
                    }

                    // Skip hidden files
                    if should_skip_file(path) {
                        continue;
                    }

                    // Check if it's a .jsonl file
                    if !has_extension(path, "jsonl") {
                        continue;
                    }

                    // Extract session ID from filename (UUID)
                    let session_id = extract_session_id_from_filename(path);

                    // Convert to canonical format and get cache path
                    let canonical_path = match Self::convert_to_canonical_file(path, &session_id) {
                        Ok(cache_path) => cache_path,
                        Err(e) => {
                            if let Err(log_err) = log_error(
                                PROVIDER_ID,
                                &format!("Failed to convert to canonical format: {}", e),
                            ) {
                                eprintln!("Logging error: {}", log_err);
                            }
                            continue;
                        }
                    };

                    // Get file size of canonical cache file
                    let file_size = get_file_size(&canonical_path).unwrap_or(0);

                    // Use "copilot-sessions" as default project name
                    // TODO: Could extract from file content if needed
                    let project_name = "copilot-sessions".to_string();

                    return Some(FileChangeEvent {
                        path: canonical_path, // Use canonical cache path, not source path
                        project_name,
                        file_size,
                        session_id,
                    });
                }
            }
            _ => {}
        }

        None
    }

    pub fn stop(&self) {
        if let Ok(mut running) = self.is_running.lock() {
            *running = false;
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping GitHub Copilot file monitoring") {
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
pub type CopilotWatcherStatus = WatcherStatus;

impl Drop for CopilotWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    // Note: Previous tests (test_extract_session_id, test_process_file_event_filters_correctly)
    // were removed because they tested private implementation details. The watcher functionality
    // is tested through integration tests and actual file system monitoring behavior.
}
