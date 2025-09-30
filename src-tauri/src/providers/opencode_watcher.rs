use super::opencode_parser::OpenCodeParser;
use crate::config::load_provider_config;
use crate::logging::{log_debug, log_error, log_info, log_warn};
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ID: &str = "opencode";
const DEBOUNCE_DURATION: Duration = Duration::from_secs(30); // 30 seconds for active sessions
const QUICK_DEBOUNCE_DURATION: Duration = Duration::from_secs(5); // 5 seconds for new sessions

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

const MIN_FILE_COUNT_CHANGE: usize = 2; // Minimum file changes to trigger re-upload

#[derive(Debug, Clone)]
pub struct SessionChangeEvent {
    pub session_id: String,
    pub project_id: String,
    pub last_modified: Instant,
    pub is_new_session: bool,
    pub affected_files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub last_modified: Instant,
    pub is_active: bool,
    pub upload_pending: bool,
    pub affected_files: HashSet<PathBuf>,
    pub last_uploaded_time: Option<Instant>,
    pub last_uploaded_size: usize, // Total size of all affected files
}

#[derive(Debug)]
pub struct OpenCodeWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: thread::JoinHandle<()>,
    upload_queue: Arc<UploadQueue>,
    is_running: Arc<Mutex<bool>>,
}

impl OpenCodeWatcher {
    pub fn new(
        projects: Vec<String>,
        upload_queue: Arc<UploadQueue>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Err(e) = log_info(PROVIDER_ID, "🔍 Starting OpenCode file monitoring") {
            eprintln!("Logging error: {}", e);
        }

        // Load provider config to get home directory
        let config = load_provider_config(PROVIDER_ID)
            .map_err(|e| format!("Failed to load provider config: {}", e))?;

        if !config.enabled {
            return Err("OpenCode provider is not enabled".into());
        }

        let home_directory = config.home_directory;
        let expanded_home = tilde(&home_directory);
        let base_path = Path::new(expanded_home.as_ref());

        if !base_path.exists() {
            return Err(format!("OpenCode home directory does not exist: {}", base_path.display()).into());
        }

        let storage_path = base_path.join("storage");
        if !storage_path.exists() {
            return Err(format!("OpenCode storage directory does not exist: {}", storage_path.display()).into());
        }

        // Create parser for session analysis
        let parser = OpenCodeParser::new(storage_path.clone());

        // Determine which projects to watch
        let projects_to_watch = if config.project_selection == "ALL" {
            // Watch all available projects
            Self::discover_all_projects(&parser)?
        } else {
            // Watch only selected projects
            let selected_projects: Vec<String> = config.selected_projects.into_iter()
                .filter(|project| projects.contains(project))
                .collect();

            if selected_projects.is_empty() {
                return Err("No valid projects selected for watching".into());
            }

            selected_projects
        };

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!("📁 Monitoring {} OpenCode projects: {}", projects_to_watch.len(), projects_to_watch.join(", ")),
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

        // Watch the entire storage directory recursively
        watcher.watch(&storage_path, RecursiveMode::Recursive)?;

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!("📂 Watching OpenCode storage: {}", storage_path.display()),
        ) {
            eprintln!("Logging error: {}", e);
        }

        let is_running = Arc::new(Mutex::new(true));
        let is_running_clone = Arc::clone(&is_running);
        let upload_queue_clone = Arc::clone(&upload_queue);

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(
                rx,
                storage_path,
                parser,
                projects_to_watch,
                upload_queue_clone,
                is_running_clone,
            );
        });

        Ok(OpenCodeWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
        })
    }

    fn discover_all_projects(parser: &OpenCodeParser) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let projects = parser.get_all_projects()
            .map_err(|e| format!("Failed to discover projects: {}", e))?;

        let project_names: Vec<String> = projects
            .into_iter()
            .map(|project| {
                Path::new(&project.worktree)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            })
            .collect();

        Ok(project_names)
    }

    fn file_event_processor(
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
        storage_path: PathBuf,
        parser: OpenCodeParser,
        projects_to_watch: Vec<String>,
        upload_queue: Arc<UploadQueue>,
        is_running: Arc<Mutex<bool>>,
    ) {
        let mut pending_sessions: HashMap<String, SessionChangeEvent> = HashMap::new();
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
                    if let Some(session_event) = Self::process_file_event(&event, &storage_path, &parser, &projects_to_watch, &session_states) {
                        // Check if this is a new session or significant change
                        let should_log = Self::should_log_event(&session_event, &session_states);

                        // Update session state immediately to prevent duplicate events
                        Self::update_session_state(&mut session_states, &session_event);

                        if should_log {
                            if session_event.is_new_session {
                                let log_message = format!(
                                    "🆕 New OpenCode session detected: {} (project: {}) → Queuing for upload",
                                    session_event.session_id, session_event.project_id
                                );
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Use debug level for routine session activity
                                let log_message = format!(
                                    "📝 OpenCode session active: {} (project: {})",
                                    session_event.session_id, session_event.project_id
                                );
                                if let Err(e) = log_debug(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            }
                        }

                        pending_sessions.insert(session_event.session_id.clone(), session_event);
                    }
                }
                Ok(Err(error)) => {
                    if let Err(e) = log_error(PROVIDER_ID, &format!("OpenCode file watcher error: {:?}", error)) {
                        eprintln!("Logging error: {}", e);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout is normal, continue to check pending sessions
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Err(e) = log_error(PROVIDER_ID, "OpenCode file watcher channel disconnected") {
                        eprintln!("Logging error: {}", e);
                    }
                    break;
                }
            }

            // Check for sessions ready to upload (smart debouncing)
            let now = Instant::now();
            let mut ready_sessions = Vec::new();

            for (session_id, session_event) in &pending_sessions {
                let debounce_duration = if session_event.is_new_session {
                    QUICK_DEBOUNCE_DURATION
                } else {
                    DEBOUNCE_DURATION
                };

                let should_upload = if session_event.is_new_session {
                    // Upload new sessions more quickly
                    now.duration_since(session_event.last_modified) >= debounce_duration
                } else {
                    // For existing sessions, check if session has become inactive
                    Self::should_upload_session(session_id, &session_states, now)
                };

                if should_upload {
                    ready_sessions.push(session_id.clone());
                }
            }

            // Process ready sessions
            for session_id in ready_sessions {
                if let Some(session_event) = pending_sessions.remove(&session_id) {
                    // Mark session as uploaded and track upload metadata
                    if let Some(session_state) = session_states.get_mut(&session_event.session_id) {
                        session_state.upload_pending = true;
                        session_state.last_uploaded_time = Some(now);
                        session_state.last_uploaded_size = session_state.affected_files.len();
                    }

                    // Parse the session and create upload
                    match parser.parse_session(&session_event.session_id) {
                        Ok(parsed_session) => {
                            if let Err(e) = upload_queue.add_session_content(
                                PROVIDER_ID,
                                &parsed_session.project_name,
                                &parsed_session.session_id,
                                parsed_session.jsonl_content,
                            ) {
                                if let Err(log_err) = log_error(
                                    PROVIDER_ID,
                                    &format!("✗ Failed to queue OpenCode session {} for upload: {}", session_event.session_id, e),
                                ) {
                                    eprintln!("Logging error: {}", log_err);
                                }
                            } else {
                                if let Err(e) = log_info(
                                    PROVIDER_ID,
                                    &format!("📤 OpenCode session {} queued for upload (project: {})", session_event.session_id, session_event.project_id),
                                ) {
                                    eprintln!("Logging error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            if let Err(log_err) = log_error(
                                PROVIDER_ID,
                                &format!("✗ Failed to parse OpenCode session {}: {}", session_event.session_id, e),
                            ) {
                                eprintln!("Logging error: {}", log_err);
                            }
                        }
                    }
                }
            }

            // Clean up old session states
            Self::cleanup_old_sessions(&mut session_states, now);
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 OpenCode file monitoring stopped") {
            eprintln!("Logging error: {}", e);
        }
    }

    fn process_file_event(
        event: &Event,
        storage_path: &Path,
        parser: &OpenCodeParser,
        projects_to_watch: &[String],
        session_states: &HashMap<String, SessionState>,
    ) -> Option<SessionChangeEvent> {
        // Only process create/modify events
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    // Skip hidden files (starting with .)
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if file_name.starts_with('.') {
                            continue;
                        }
                    }

                    // Check if it's a JSON file in the storage directory
                    if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                        continue;
                    }

                    if !path.starts_with(storage_path) {
                        continue;
                    }

                    // Extract relative path from storage
                    if let Ok(relative_path) = path.strip_prefix(storage_path) {
                        let components: Vec<_> = relative_path.components().collect();

                        // Process different types of file changes
                        match components.get(0).and_then(|c| c.as_os_str().to_str()) {
                            Some("part") => {
                                // Part file changed: part/{messageId}/{partId}.json
                                if let Some(session_id) = parser.get_session_for_part(path) {
                                    if let Some(project_id) = parser.get_project_for_session(&session_id) {
                                        // Check if this project is being watched
                                        if let Ok(project) = parser.get_all_projects() {
                                            for proj in project {
                                                if proj.id == project_id {
                                                    let project_name = Path::new(&proj.worktree)
                                                        .file_name()
                                                        .and_then(|name| name.to_str())
                                                        .unwrap_or("unknown");

                                                    if projects_to_watch.contains(&project_name.to_string()) {
                                                        return Some(SessionChangeEvent {
                                                            session_id: session_id.clone(),
                                                            project_id,
                                                            last_modified: Instant::now(),
                                                            is_new_session: Self::is_new_session(&session_id, path, session_states),
                                                            affected_files: vec![path.clone()],
                                                        });
                                                    }
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Some("message") => {
                                // Message file changed: message/{sessionId}/{messageId}.json
                                if components.len() >= 3 {
                                    if let Some(session_id) = components.get(1).and_then(|c| c.as_os_str().to_str()) {
                                        if let Some(project_id) = parser.get_project_for_session(session_id) {
                                            return Some(SessionChangeEvent {
                                                session_id: session_id.to_string(),
                                                project_id,
                                                last_modified: Instant::now(),
                                                is_new_session: Self::is_new_session(session_id, path, session_states),
                                                affected_files: vec![path.clone()],
                                            });
                                        }
                                    }
                                }
                            }
                            Some("session") => {
                                // Session file changed: session/{projectId}/{sessionId}.json
                                if components.len() >= 3 {
                                    if let Some(project_id) = components.get(1).and_then(|c| c.as_os_str().to_str()) {
                                        if let Some(session_id) = components.get(2)
                                            .and_then(|c| c.as_os_str().to_str())
                                            .and_then(|s| s.strip_suffix(".json")) {
                                            return Some(SessionChangeEvent {
                                                session_id: session_id.to_string(),
                                                project_id: project_id.to_string(),
                                                last_modified: Instant::now(),
                                                is_new_session: Self::is_new_session(session_id, path, session_states),
                                                affected_files: vec![path.clone()],
                                            });
                                        }
                                    }
                                }
                            }
                            _ => {
                                // Ignore other file types (project files are handled by project scanner)
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn is_new_session(session_id: &str, path: &Path, session_states: &HashMap<String, SessionState>) -> bool {
        // First check if we've already seen this session
        if session_states.contains_key(session_id) {
            return false; // Already tracking this session
        }

        // Check if this file is new by looking at file size
        // A new session typically starts with a small file size
        if let Ok(metadata) = std::fs::metadata(path) {
            let file_size = metadata.len();
            // Consider it a new session if file is small (less than 1KB)
            file_size < 1024
        } else {
            true // If we can't read metadata, assume it's new
        }
    }

    fn should_log_event(session_event: &SessionChangeEvent, session_states: &HashMap<String, SessionState>) -> bool {
        match session_states.get(&session_event.session_id) {
            Some(_existing_state) => {
                // Only log if this is a new session
                session_event.is_new_session
            },
            None => {
                // Only log if this is actually a new session
                session_event.is_new_session
            }
        }
    }

    fn update_session_state(session_states: &mut HashMap<String, SessionState>, session_event: &SessionChangeEvent) {
        match session_states.get_mut(&session_event.session_id) {
            Some(existing_state) => {
                // Track old file count for change detection
                let old_file_count = existing_state.affected_files.len();

                // Update existing session state
                existing_state.last_modified = session_event.last_modified;
                existing_state.is_active = true;
                for file in &session_event.affected_files {
                    existing_state.affected_files.insert(file.clone());
                }

                // Smart re-upload logic: clear upload_pending if conditions met
                if existing_state.upload_pending {
                    let new_file_count = existing_state.affected_files.len();
                    let file_count_changed = new_file_count.saturating_sub(old_file_count) >= MIN_FILE_COUNT_CHANGE;

                    let should_allow_reupload = if let Some(last_uploaded_time) = existing_state.last_uploaded_time {
                        // Check if cooldown has elapsed OR file count changed significantly
                        let cooldown_elapsed = session_event.last_modified.duration_since(last_uploaded_time) >= RE_UPLOAD_COOLDOWN;

                        cooldown_elapsed || file_count_changed
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
                let mut affected_files = HashSet::new();
                for file in &session_event.affected_files {
                    affected_files.insert(file.clone());
                }

                let session_state = SessionState {
                    last_modified: session_event.last_modified,
                    is_active: true,
                    upload_pending: false,
                    affected_files,
                    last_uploaded_time: None,
                    last_uploaded_size: 0,
                };
                session_states.insert(session_event.session_id.clone(), session_state);
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

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping OpenCode file monitoring") {
            eprintln!("Logging error: {}", e);
        }
    }

    pub fn get_status(&self) -> OpenCodeWatcherStatus {
        let is_running = if let Ok(running) = self.is_running.lock() {
            *running
        } else {
            false
        };

        let upload_status = self.upload_queue.get_status();

        OpenCodeWatcherStatus {
            is_running,
            pending_uploads: upload_status.pending,
            processing_uploads: upload_status.processing,
            failed_uploads: upload_status.failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeWatcherStatus {
    pub is_running: bool,
    pub pending_uploads: usize,
    pub processing_uploads: usize,
    pub failed_uploads: usize,
}

impl Drop for OpenCodeWatcher {
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
    fn test_is_new_session() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.json");
        let session_id = "test-session";

        // Test with empty session states
        let mut session_states = HashMap::new();

        // Create a small file - should be considered new
        fs::write(&file_path, "{}").unwrap();
        assert!(OpenCodeWatcher::is_new_session(session_id, &file_path, &session_states));

        // Create a larger file - should not be considered new based on size
        let large_content = "x".repeat(2000);
        fs::write(&file_path, large_content).unwrap();
        assert!(!OpenCodeWatcher::is_new_session(session_id, &file_path, &session_states));

        // Add session to states - should not be considered new even if file is small
        session_states.insert(session_id.to_string(), SessionState {
            last_modified: Instant::now(),
            is_active: true,
            upload_pending: false,
            affected_files: HashSet::new(),
            last_uploaded_time: None,
            last_uploaded_size: 0,
        });
        fs::write(&file_path, "{}").unwrap();
        assert!(!OpenCodeWatcher::is_new_session(session_id, &file_path, &session_states));
    }

    #[test]
    fn test_process_file_event_skips_hidden_files() {
        use notify::event::CreateKind;

        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path();

        // Create session directory structure
        let session_dir = storage_path.join("session").join("project1");
        fs::create_dir_all(&session_dir).unwrap();

        // Create a hidden file
        let hidden_file = session_dir.join(".tmpABCDEF.json");
        fs::write(&hidden_file, "{}").unwrap();

        // Create a normal file
        let normal_file = session_dir.join("session-123.json");
        fs::write(&normal_file, "{}").unwrap();

        // Create a minimal parser (won't actually parse, just checking file filtering)
        let parser = OpenCodeParser::new(storage_path.to_path_buf());
        let projects_to_watch = vec!["project1".to_string()];
        let session_states = HashMap::new();

        // Test hidden file is ignored
        let hidden_event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![hidden_file.clone()],
            attrs: Default::default(),
        };
        let result = OpenCodeWatcher::process_file_event(&hidden_event, storage_path, &parser, &projects_to_watch, &session_states);
        assert!(result.is_none(), "Hidden file should be ignored");

        // Test normal file would be processed (will fail to find project, but the file isn't filtered out)
        let normal_event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![normal_file.clone()],
            attrs: Default::default(),
        };
        let _result = OpenCodeWatcher::process_file_event(&normal_event, storage_path, &parser, &projects_to_watch, &session_states);
        // Result might be None if project lookup fails, but that's OK - we're just testing file filtering
        // The important thing is it didn't get filtered out like the hidden file
    }
}