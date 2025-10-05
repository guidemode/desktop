use crate::config::load_provider_config;
use crate::logging::{log_debug, log_error, log_info, log_warn};
use crate::providers::db_helpers::insert_session_immediately;
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ID: &str = "claude-code";

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
pub struct ClaudeWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: thread::JoinHandle<()>,
    upload_queue: Arc<UploadQueue>,
    is_running: Arc<Mutex<bool>>,
}

impl ClaudeWatcher {
    pub fn new(
        projects: Vec<String>,
        upload_queue: Arc<UploadQueue>,
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
            Config::default().with_poll_interval(Duration::from_secs(2)),
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

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(
                rx,
                projects_path_clone,
                upload_queue_clone,
                is_running_clone,
            );
        });

        Ok(ClaudeWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
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
        is_running: Arc<Mutex<bool>>,
    ) {
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
                    if let Some(file_event) =
                        Self::process_file_event(&event, &projects_path, &session_states)
                    {
                        // Check if this is a new session or significant change (before updating state)
                        let should_log = Self::should_log_event(&file_event, &session_states);

                        // INSERT TO DATABASE IMMEDIATELY (no debounce) - always for local storage
                        if let Err(e) = insert_session_immediately(
                            PROVIDER_ID,
                            &file_event.project_name,
                            &file_event.session_id,
                            &file_event.path,
                            file_event.file_size,
                        ) {
                            if let Err(log_err) = log_error(
                                PROVIDER_ID,
                                &format!("Failed to insert session to database: {}", e),
                            ) {
                                eprintln!("Logging error: {}", log_err);
                            }
                        }

                        // Update session state immediately to prevent duplicate events
                        Self::update_session_state(&mut session_states, &file_event);

                        if should_log {
                            if file_event.is_new_session {
                                let log_message = format!(
                                    "🆕 New Claude Code session detected: {} → Saved to database",
                                    file_event.session_id
                                );
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Use debug level for routine session activity
                                let log_message = format!(
                                    "📝 Claude Code session active: {} (size: {} bytes)",
                                    file_event.session_id, file_event.file_size
                                );
                                if let Err(e) = log_debug(PROVIDER_ID, &log_message) {
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

                    // Extract project name from path
                    if let Some(project_name) = Self::extract_project_name(path, projects_path) {
                        // Get file size and session ID
                        let file_size = Self::get_file_size(path).unwrap_or(0);
                        let session_id = Self::extract_session_id(path);

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

    fn get_file_size(path: &Path) -> Result<u64, std::io::Error> {
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
    }

    fn extract_session_id(path: &Path) -> String {
        // Extract session ID from filename (UUID format)
        if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
            file_name.to_string()
        } else {
            "unknown".to_string()
        }
    }

    fn is_new_session(
        session_id: &str,
        _path: &Path,
        session_states: &HashMap<String, SessionState>,
    ) -> bool {
        // A session is considered new if we haven't seen it before
        // We don't check file size because sessions can grow quickly
        !session_states.contains_key(session_id)
    }

    fn should_log_event(
        file_event: &FileChangeEvent,
        session_states: &HashMap<String, SessionState>,
    ) -> bool {
        match session_states.get(&file_event.session_id) {
            Some(existing_state) => {
                // Only log if significant size change or new session
                file_event.is_new_session
                    || file_event
                        .file_size
                        .saturating_sub(existing_state.last_size)
                        >= MIN_SIZE_CHANGE_BYTES
            }
            None => {
                // Only log if this is actually a new session (small file size)
                // This prevents duplicate logging for the same session when multiple
                // file events occur before the session state is updated
                file_event.is_new_session
            }
        }
    }

    fn update_session_state(
        session_states: &mut HashMap<String, SessionState>,
        file_event: &FileChangeEvent,
    ) {
        match session_states.get_mut(&file_event.session_id) {
            Some(existing_state) => {
                // Update existing session state
                existing_state.last_modified = file_event.last_modified;
                existing_state.last_size = file_event.file_size;
                existing_state.is_active = true;

                // Smart re-upload logic: clear upload_pending if conditions met
                if existing_state.upload_pending {
                    let should_allow_reupload =
                        if let Some(last_uploaded_time) = existing_state.last_uploaded_time {
                            // Check if cooldown has elapsed OR size changed significantly
                            let cooldown_elapsed =
                                file_event.last_modified.duration_since(last_uploaded_time)
                                    >= RE_UPLOAD_COOLDOWN;
                            let size_changed_significantly = file_event
                                .file_size
                                .saturating_sub(existing_state.last_uploaded_size)
                                >= MIN_SIZE_CHANGE_BYTES;

                            cooldown_elapsed || size_changed_significantly
                        } else {
                            // No last upload time recorded, allow re-upload
                            true
                        };

                    if should_allow_reupload {
                        existing_state.upload_pending = false;
                    }
                }
            }
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

    pub fn stop(&self) {
        if let Ok(mut running) = self.is_running.lock() {
            *running = false;
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping Claude Code file monitoring") {
            eprintln!("Logging error: {}", e);
        }
    }

    pub fn get_status(&self) -> ClaudeWatcherStatus {
        let is_running = if let Ok(running) = self.is_running.lock() {
            *running
        } else {
            false
        };

        let upload_status = self.upload_queue.get_status();

        ClaudeWatcherStatus {
            is_running,
            pending_uploads: upload_status.pending,
            processing_uploads: upload_status.processing,
            failed_uploads: upload_status.failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeWatcherStatus {
    pub is_running: bool,
    pub pending_uploads: usize,
    pub processing_uploads: usize,
    pub failed_uploads: usize,
}

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
        let session_states = HashMap::new();
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
