use crate::config::load_provider_config;
use crate::logging::{log_debug, log_error, log_info};
use crate::providers::copilot_parser::{
    detect_project_and_cwd_from_timeline, load_copilot_config, CopilotSession,
};
use crate::providers::copilot_snapshot::SnapshotManager;
use crate::providers::db_helpers::insert_session_immediately;
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ID: &str = "github-copilot";

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

        let session_dir = base_path.join("history-session-state");
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
            Config::default().with_poll_interval(Duration::from_secs(2)),
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

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(rx, session_dir, upload_queue_clone, is_running_clone);
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
        is_running: Arc<Mutex<bool>>,
    ) {
        // Track SNAPSHOT states, not source file states
        let mut snapshot_states: HashMap<String, SessionState> = HashMap::new();

        // Create snapshot manager
        let snapshot_manager = match SnapshotManager::new() {
            Ok(manager) => manager,
            Err(e) => {
                if let Err(log_err) = log_error(
                    PROVIDER_ID,
                    &format!("Failed to create snapshot manager: {}", e),
                ) {
                    eprintln!("Logging error: {}", log_err);
                }
                return;
            }
        };

        loop {
            // Check if we should continue running
            {
                if let Ok(running) = is_running.lock() {
                    if !*running {
                        break;
                    }
                }
            }

            // Load current provider config to check project selection
            let provider_config = match load_provider_config(PROVIDER_ID) {
                Ok(config) => config,
                Err(e) => {
                    if let Err(log_err) = log_error(
                        PROVIDER_ID,
                        &format!("Failed to load provider config: {}", e),
                    ) {
                        eprintln!("Logging error: {}", log_err);
                    }
                    continue;
                }
            };

            // Process file system events with timeout
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(event)) => {
                    // Detect if this is a copilot source file change
                    if let Some(source_file) =
                        Self::detect_copilot_source_file(&event, &session_dir)
                    {
                        // Translate to snapshot event
                        match Self::handle_copilot_file_change(
                            &source_file,
                            &snapshot_manager,
                            &provider_config,
                        ) {
                            Ok(Some(snapshot_event)) => {
                                // Process the snapshot event (not the source file event!)
                                Self::process_snapshot_event(snapshot_event, &mut snapshot_states);
                            }
                            Ok(None) => {
                                // No new entries, skip
                            }
                            Err(e) => {
                                if let Err(log_err) = log_error(
                                    PROVIDER_ID,
                                    &format!(
                                        "Failed to process copilot file {}: {}",
                                        source_file.display(),
                                        e
                                    ),
                                ) {
                                    eprintln!("Logging error: {}", log_err);
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

    fn detect_copilot_source_file(event: &Event, session_dir: &Path) -> Option<PathBuf> {
        // Only process write events for session files
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    // Check if it's in the session directory
                    if !path.starts_with(session_dir) {
                        continue;
                    }

                    // Skip hidden files (starting with .)
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if file_name.starts_with('.') {
                            continue;
                        }

                        // Only process session files (start with "session_" and end with .json)
                        if !file_name.starts_with("session_") {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    // Check if it's a .json file
                    if let Some(extension) = path.extension() {
                        if extension != "json" {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    return Some(path.clone());
                }
            }
            _ => {}
        }

        None
    }

    fn handle_copilot_file_change(
        source_file: &Path,
        snapshot_manager: &SnapshotManager,
        provider_config: &crate::config::ProviderConfig,
    ) -> Result<Option<FileChangeEvent>, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Parse the copilot session file
        let content = fs::read_to_string(source_file)?;
        let copilot_session: CopilotSession = serde_json::from_str(&content)?;
        let timeline = &copilot_session.timeline;

        // Skip if timeline is empty and we haven't seen this file before
        if timeline.is_empty() {
            return Ok(None);
        }

        // 2. Detect project name and cwd from timeline entries
        let (project_name, project_cwd) = match load_copilot_config() {
            Ok(config) => {
                if !config.trusted_folders.is_empty() {
                    // Try to detect project from timeline
                    if let Some((name, cwd)) =
                        detect_project_and_cwd_from_timeline(timeline, &config.trusted_folders)
                    {
                        (name, Some(cwd))
                    } else {
                        ("copilot-sessions".to_string(), None)
                    }
                } else {
                    ("copilot-sessions".to_string(), None)
                }
            }
            Err(e) => {
                if let Err(log_err) = log_debug(
                    PROVIDER_ID,
                    &format!(
                        "Failed to load copilot config: {}, using default project name",
                        e
                    ),
                ) {
                    eprintln!("Logging error: {}", log_err);
                }
                ("copilot-sessions".to_string(), None)
            }
        };

        // 3. Check if this project should be processed based on config
        if provider_config.project_selection != "ALL" {
            // Only process selected projects
            if !provider_config.selected_projects.contains(&project_name) {
                // Project not selected - ignore it completely
                if let Err(e) = log_debug(
                    PROVIDER_ID,
                    &format!(
                        "⏭️ Skipping project '{}' (not in selected projects)",
                        project_name
                    ),
                ) {
                    eprintln!("Logging error: {}", e);
                }
                return Ok(None);
            }
        }

        // 4. Load metadata (with file lock)
        let (mut metadata, lock_file) = snapshot_manager.load_metadata_locked()?;

        // 5. Get or check session entry
        let file_name = source_file
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid filename")?;

        let file_size = fs::metadata(source_file)?.len();

        // 6. Check for truncation or first-time
        let snapshot_event = if let Some(session) = metadata.sessions.get_mut(file_name) {
            // Existing session - check for truncation
            if SnapshotManager::is_truncated(session, timeline.len(), file_size) {
                // TRUNCATION DETECTED - close current and create new snapshot
                session.close_active_snapshot()?;

                let new_snapshot_id = uuid::Uuid::new_v4();
                let snapshot_path = snapshot_manager.create_snapshot_file(
                    new_snapshot_id,
                    timeline,
                    project_cwd.as_deref(),
                )?;
                session.add_snapshot(new_snapshot_id, timeline.len(), file_size)?;

                // Save metadata
                snapshot_manager.save_metadata_atomic(&metadata, lock_file)?;

                if let Err(e) = log_info(
                    PROVIDER_ID,
                    &format!(
                        "🔄 Copilot session truncated, new snapshot: {}",
                        new_snapshot_id
                    ),
                ) {
                    eprintln!("Logging error: {}", e);
                }

                // Return event for NEW snapshot
                Some(Self::create_snapshot_event(
                    &snapshot_path,
                    new_snapshot_id,
                    true,
                    &project_name,
                ))
            } else {
                // Normal update - rewrite entire snapshot file
                // This captures updates to existing entries (e.g., tool call results)
                let active = session.get_active_snapshot_mut()?;
                let last_count = active.last_timeline_count;
                let snapshot_id = active.snapshot_id; // Copy the UUID before borrowing ends

                if timeline.len() >= last_count {
                    // Rewrite the entire file with the full timeline
                    let snapshot_path = snapshot_manager.append_to_snapshot_file(
                        snapshot_id,
                        timeline,
                        project_cwd.as_deref(),
                    )?;

                    active.last_timeline_count = timeline.len();
                    active.last_updated = chrono::Utc::now().to_rfc3339();
                    active.last_source_file_size = file_size;

                    // Save metadata
                    snapshot_manager.save_metadata_atomic(&metadata, lock_file)?;

                    // Return event for UPDATED snapshot
                    Some(Self::create_snapshot_event(
                        &snapshot_path,
                        snapshot_id,
                        false,
                        &project_name,
                    ))
                } else {
                    // No changes
                    snapshot_manager.save_metadata_atomic(&metadata, lock_file)?;
                    None
                }
            }
        } else {
            // First time seeing this file - create initial snapshot
            let snapshot_id = snapshot_manager.get_or_create_session(
                &mut metadata,
                source_file,
                &copilot_session.session_id,
                &copilot_session.start_time,
                timeline,
                project_cwd.as_deref(),
            )?;

            // Save metadata
            snapshot_manager.save_metadata_atomic(&metadata, lock_file)?;

            let snapshot_path = snapshot_manager.get_snapshot_path(snapshot_id);

            if let Err(e) = log_info(
                PROVIDER_ID,
                &format!("🆕 New Copilot snapshot created: {}", snapshot_id),
            ) {
                eprintln!("Logging error: {}", e);
            }

            // Return event for NEW snapshot
            Some(Self::create_snapshot_event(
                &snapshot_path,
                snapshot_id,
                true,
                &project_name,
            ))
        };

        Ok(snapshot_event)
    }

    fn create_snapshot_event(
        snapshot_path: &Path,
        snapshot_id: uuid::Uuid,
        is_new_session: bool,
        project_name: &str,
    ) -> FileChangeEvent {
        let file_size = fs::metadata(snapshot_path).map(|m| m.len()).unwrap_or(0);

        FileChangeEvent {
            path: snapshot_path.to_path_buf(), // SNAPSHOT path, not source!
            project_name: project_name.to_string(),
            last_modified: Instant::now(),
            file_size,
            session_id: snapshot_id.to_string(), // SNAPSHOT UUID, not source session id!
            is_new_session,
        }
    }

    fn process_snapshot_event(
        snapshot_event: FileChangeEvent,
        snapshot_states: &mut HashMap<String, SessionState>,
    ) {
        let should_log = Self::should_log_event(&snapshot_event, snapshot_states);

        // INSERT SNAPSHOT TO DATABASE (snapshot path, snapshot id)
        if let Err(e) = insert_session_immediately(
            PROVIDER_ID,
            &snapshot_event.project_name,
            &snapshot_event.session_id, // snapshot UUID
            &snapshot_event.path,       // snapshot .jsonl path
            snapshot_event.file_size,
        ) {
            if let Err(log_err) = log_error(
                PROVIDER_ID,
                &format!("Failed to insert snapshot to database: {}", e),
            ) {
                eprintln!("Logging error: {}", log_err);
            }
        }

        // Update snapshot state tracking
        Self::update_session_state(snapshot_states, &snapshot_event);

        // Log events
        if should_log {
            if snapshot_event.is_new_session {
                if let Err(e) = log_info(
                    PROVIDER_ID,
                    &format!(
                        "🆕 New Copilot snapshot saved to database: {} ({} bytes)",
                        snapshot_event.session_id, snapshot_event.file_size
                    ),
                ) {
                    eprintln!("Logging error: {}", e);
                }
            } else {
                if let Err(e) = log_debug(
                    PROVIDER_ID,
                    &format!(
                        "📝 Copilot snapshot updated: {} ({} bytes)",
                        snapshot_event.session_id, snapshot_event.file_size
                    ),
                ) {
                    eprintln!("Logging error: {}", e);
                }
            }
        }
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
                // Only log if this is actually a new session
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

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping GitHub Copilot file monitoring") {
            eprintln!("Logging error: {}", e);
        }
    }

    pub fn get_status(&self) -> CopilotWatcherStatus {
        let is_running = if let Ok(running) = self.is_running.lock() {
            *running
        } else {
            false
        };

        let upload_status = self.upload_queue.get_status();

        CopilotWatcherStatus {
            is_running,
            pending_uploads: upload_status.pending,
            processing_uploads: upload_status.processing,
            failed_uploads: upload_status.failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotWatcherStatus {
    pub is_running: bool,
    pub pending_uploads: usize,
    pub processing_uploads: usize,
    pub failed_uploads: usize,
}

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
