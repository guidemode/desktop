use crate::config::load_provider_config;
use crate::events::{EventBus, SessionEventPayload};
use crate::logging::{log_debug, log_error, log_info, log_warn};
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

const PROVIDER_ID: &str = "claude-code";

#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub project_name: String,
    pub file_size: u64,
    pub session_id: String,
}

#[derive(Debug)]
pub struct ClaudeWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: thread::JoinHandle<()>,
    upload_queue: Arc<UploadQueue>,
    is_running: Arc<Mutex<bool>>,
    event_bus: EventBus,
}

impl ClaudeWatcher {
    pub fn new(
        projects: Vec<String>,
        upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Err(e) = log_info(PROVIDER_ID, "🔍 Starting Claude Code file monitoring") {
            eprintln!("Logging error: {}", e);
        }

        // Load provider config to get home directory
        let config = load_provider_config(PROVIDER_ID)
            .map_err(|e| format!("Failed to load provider config: {}", e))?;

        if !config.enabled {
            return Err("Claude Code provider is not enabled".into());
        }

        let home_directory = config.home_directory;
        let expanded_home = tilde(&home_directory);
        let base_path = Path::new(expanded_home.as_ref());

        if !base_path.exists() {
            return Err(format!(
                "Claude Code home directory does not exist: {}",
                base_path.display()
            )
            .into());
        }

        let projects_path = base_path.join("projects");
        if !projects_path.exists() {
            return Err(format!(
                "Claude Code projects directory does not exist: {}",
                projects_path.display()
            )
            .into());
        }

        // Determine which projects to watch
        let projects_to_watch = if config.project_selection == "ALL" {
            // Watch all available projects
            Self::discover_all_projects(&projects_path)?
        } else {
            // Watch only selected projects
            let selected_projects: Vec<String> = config
                .selected_projects
                .into_iter()
                .filter(|project| projects.contains(project))
                .collect();

            if selected_projects.is_empty() {
                return Err("No valid projects selected for watching".into());
            }

            selected_projects
        };

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📁 Monitoring {} Claude Code projects: {}",
                projects_to_watch.len(),
                projects_to_watch.join(", ")
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

        // Watch each selected project directory
        for project_name in &projects_to_watch {
            let project_path = projects_path.join(project_name);
            if project_path.exists() && project_path.is_dir() {
                watcher.watch(&project_path, RecursiveMode::Recursive)?;
                if let Err(e) = log_info(
                    PROVIDER_ID,
                    &format!(
                        "📂 Watching Claude Code project: {}",
                        project_path.display()
                    ),
                ) {
                    eprintln!("Logging error: {}", e);
                }
            } else {
                if let Err(e) = log_warn(
                    PROVIDER_ID,
                    &format!("⚠ Project directory not found: {}", project_path.display()),
                ) {
                    eprintln!("Logging error: {}", e);
                }
            }
        }

        let is_running = Arc::new(Mutex::new(true));
        let is_running_clone = Arc::clone(&is_running);
        let upload_queue_clone = Arc::clone(&upload_queue);
        let projects_path_clone = projects_path.clone();
        let event_bus_clone = event_bus.clone();

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(
                rx,
                projects_path_clone,
                upload_queue_clone,
                event_bus_clone,
                is_running_clone,
            );
        });

        Ok(ClaudeWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
            event_bus,
        })
    }

    fn discover_all_projects(
        projects_path: &Path,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let entries = std::fs::read_dir(projects_path)
            .map_err(|e| format!("Failed to read projects directory: {}", e))?;

        let mut projects = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    projects.push(name.to_string());
                }
            }
        }

        Ok(projects)
    }

    fn file_event_processor(
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
        projects_path: PathBuf,
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
                        Self::process_file_event(&event, &projects_path, &session_states)
                    {
                        // Check if this is a new session (before get_or_create)
                        let is_new_session = !session_states.contains(&file_event.session_id);

                        // Get or create session state
                        let state = session_states.get_or_create(
                            &file_event.session_id,
                            file_event.file_size,
                        );
                        let should_log = state.should_log(
                            file_event.file_size,
                            MIN_SIZE_CHANGE_BYTES,
                            is_new_session,
                        );

                        // Publish SessionChanged event to event bus
                        // DatabaseEventHandler will call db_helpers which does smart insert-or-update
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

                        if should_log {
                            if is_new_session {
                                let log_message = format!(
                                    "🆕 New Claude Code session detected: {}",
                                    file_event.session_id
                                );
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Log session updates at info level
                                let log_message = format!(
                                    "📝 Claude Code session changed: {} (size: {} bytes)",
                                    file_event.session_id, file_event.file_size
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

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Claude Code file monitoring stopped") {
            eprintln!("Logging error: {}", e);
        }
    }

    fn process_file_event(
        event: &Event,
        projects_path: &Path,
        session_states: &SessionStateManager,
    ) -> Option<FileChangeEvent> {
        // Only process write events for .jsonl files
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    // Skip hidden files
                    if should_skip_file(path) {
                        continue;
                    }

                    // Check if it's a .jsonl file
                    if !has_extension(path, "jsonl") {
                        continue;
                    }

                    // Extract project name from path
                    if let Some(project_name) = Self::extract_project_name(path, projects_path) {
                        // Get file size and session ID
                        let file_size = get_file_size(path).unwrap_or(0);
                        let session_id = extract_session_id_from_filename(path);

                        return Some(FileChangeEvent {
                            path: path.clone(),
                            project_name,
                            file_size,
                            session_id: session_id.clone(),
                        });
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn extract_project_name(file_path: &Path, projects_path: &Path) -> Option<String> {
        // Get the relative path from projects directory
        if let Ok(relative_path) = file_path.strip_prefix(projects_path) {
            // The first component should be the project name
            if let Some(first_component) = relative_path.components().next() {
                if let Some(project_name) = first_component.as_os_str().to_str() {
                    return Some(project_name.to_string());
                }
            }
        }
        None
    }


    pub fn stop(&self) {
        if let Ok(mut running) = self.is_running.lock() {
            *running = false;
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping Claude Code file monitoring") {
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
pub type ClaudeWatcherStatus = WatcherStatus;

impl Drop for ClaudeWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_extract_project_name() {
        let projects_dir = Path::new("/home/user/.claude/projects");
        let file_path = Path::new("/home/user/.claude/projects/my-project/session/file.jsonl");

        let project_name = ClaudeWatcher::extract_project_name(file_path, projects_dir);
        assert_eq!(project_name, Some("my-project".to_string()));
    }

    #[test]
    fn test_discover_all_projects() {
        let temp_dir = tempdir().unwrap();
        let projects_path = temp_dir.path();

        // Create some project directories
        fs::create_dir_all(projects_path.join("project1")).unwrap();
        fs::create_dir_all(projects_path.join("project2")).unwrap();
        fs::create_dir_all(projects_path.join("project3")).unwrap();

        // Create a file (should be ignored)
        fs::write(projects_path.join("not_a_project.txt"), "content").unwrap();

        let projects = ClaudeWatcher::discover_all_projects(projects_path).unwrap();

        assert_eq!(projects.len(), 3);
        assert!(projects.contains(&"project1".to_string()));
        assert!(projects.contains(&"project2".to_string()));
        assert!(projects.contains(&"project3".to_string()));
    }

    #[test]
    fn test_process_file_event_skips_hidden_files() {
        use notify::event::{CreateKind, ModifyKind};

        let temp_dir = tempdir().unwrap();
        let projects_path = temp_dir.path();

        // Create project directory structure
        let project_path = projects_path.join("test-project");
        fs::create_dir_all(&project_path).unwrap();

        // Create a hidden file
        let hidden_file = project_path.join(".tmpABCDEF.jsonl");
        fs::write(&hidden_file, r#"{"timestamp":"2025-01-01T10:00:00.000Z"}"#).unwrap();

        // Create a normal file
        let normal_file = project_path.join("session-123.jsonl");
        fs::write(&normal_file, r#"{"timestamp":"2025-01-01T10:00:00.000Z"}"#).unwrap();

        // Test hidden file is ignored
        let hidden_event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![hidden_file.clone()],
            attrs: Default::default(),
        };
        let session_states = SessionStateManager::new();
        let result =
            ClaudeWatcher::process_file_event(&hidden_event, projects_path, &session_states);
        assert!(result.is_none(), "Hidden file should be ignored");

        // Test normal file is processed
        let normal_event = Event {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![normal_file.clone()],
            attrs: Default::default(),
        };
        let result =
            ClaudeWatcher::process_file_event(&normal_event, projects_path, &session_states);
        assert!(result.is_some(), "Normal file should be processed");
        assert_eq!(result.unwrap().session_id, "session-123");
    }
}
