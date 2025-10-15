use crate::config::load_provider_config;
use crate::logging::{log_debug, log_error, log_info};
use crate::providers::common::{
    get_file_size, SessionStateManager, WatcherStatus, EVENT_TIMEOUT, FILE_WATCH_POLL_INTERVAL,
    MIN_SIZE_CHANGE_BYTES,
};
use crate::providers::copilot_parser::{
    detect_project_and_cwd_from_timeline, load_copilot_config, CopilotSession,
};
use crate::providers::copilot_snapshot::SnapshotManager;
use crate::events::{EventBus, SessionEventPayload};
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use shellexpand::tilde;
use std::fs;
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
    event_bus: EventBus,
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
            Self::file_event_processor(rx, session_dir, upload_queue_clone, event_bus_clone, is_running_clone);
        });

        Ok(CopilotWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
            event_bus,
        })
    }

    fn file_event_processor(
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
        session_dir: PathBuf,
        _upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
        is_running: Arc<Mutex<bool>>,
    ) {
        // Track SNAPSHOT states, not source file states
        let mut snapshot_states = SessionStateManager::new();

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
            match rx.recv_timeout(EVENT_TIMEOUT) {
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
                                Self::process_snapshot_event(snapshot_event, &mut snapshot_states, &event_bus);
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
                &project_name,
            ))
        };

        Ok(snapshot_event)
    }

    fn create_snapshot_event(
        snapshot_path: &Path,
        snapshot_id: uuid::Uuid,
        project_name: &str,
    ) -> FileChangeEvent {
        let file_size = get_file_size(snapshot_path).unwrap_or(0);

        FileChangeEvent {
            path: snapshot_path.to_path_buf(), // SNAPSHOT path, not source!
            project_name: project_name.to_string(),
            file_size,
            session_id: snapshot_id.to_string(), // SNAPSHOT UUID, not source session id!
        }
    }

    fn process_snapshot_event(
        snapshot_event: FileChangeEvent,
        snapshot_states: &mut SessionStateManager,
        event_bus: &EventBus,
    ) {
        // Check if this is a new session (before get_or_create)
        let is_new_session = !snapshot_states.contains(&snapshot_event.session_id);

        // Get or create session state
        let state = snapshot_states.get_or_create(
            &snapshot_event.session_id,
            snapshot_event.file_size,
        );
        let should_log = state.should_log(
            snapshot_event.file_size,
            MIN_SIZE_CHANGE_BYTES,
            is_new_session,
        );

        // Publish SessionChanged event to event bus
        // DatabaseEventHandler will call db_helpers which does smart insert-or-update
        let payload = SessionEventPayload::SessionChanged {
            session_id: snapshot_event.session_id.clone(),
            project_name: snapshot_event.project_name.clone(),
            file_path: snapshot_event.path.clone(),
            file_size: snapshot_event.file_size,
        };

        if let Err(e) = event_bus.publish(PROVIDER_ID, payload) {
            if let Err(log_err) = log_error(
                PROVIDER_ID,
                &format!("Failed to publish session event: {}", e),
            ) {
                eprintln!("Logging error: {}", log_err);
            }
        }

        // Update snapshot state tracking
        state.update(snapshot_event.file_size);

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
                        snapshot_event.session_id
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
                        snapshot_event.session_id, snapshot_event.file_size
                    ),
                ) {
                    eprintln!("Logging error: {}", e);
                }
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
