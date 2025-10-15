use crate::config::load_provider_config;
use crate::events::{EventBus, SessionEventPayload};
use crate::logging::{log_debug, log_error, log_info};
use crate::providers::common::{
    get_file_size, has_extension, should_skip_file, SessionStateManager, WatcherStatus,
    EVENT_TIMEOUT, FILE_WATCH_POLL_INTERVAL, MIN_SIZE_CHANGE_BYTES,
};
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, PollWatcher, RecursiveMode, Watcher};
use shellexpand::tilde;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

const PROVIDER_ID: &str = "codex";

#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub project_name: String,
    pub file_size: u64,
    pub session_id: String,
}

#[derive(Debug)]
pub struct CodexWatcher {
    _watcher: PollWatcher,
    _thread_handle: thread::JoinHandle<()>,
    upload_queue: Arc<UploadQueue>,
    is_running: Arc<Mutex<bool>>,
    event_bus: EventBus,
}

impl CodexWatcher {
    pub fn new(
        _projects: Vec<String>,
        upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
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
            return Err(format!(
                "Codex home directory does not exist: {}",
                base_path.display()
            )
            .into());
        }

        let sessions_path = base_path.join("sessions");
        if !sessions_path.exists() {
            return Err(format!(
                "Codex sessions directory does not exist: {}",
                sessions_path.display()
            )
            .into());
        }

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📁 Monitoring Codex sessions directory: {}",
                sessions_path.display()
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }

        // Create file system event channel
        let (tx, rx) = mpsc::channel();

        // Create the file watcher with aggressive polling
        // Note: We use PollWatcher instead of RecommendedWatcher because Codex keeps
        // session files open with write descriptors, and macOS FSEvents doesn't
        // reliably detect writes to already-open files until they're closed/fsynced
        if let Err(e) = log_info(
            PROVIDER_ID,
            "⚙️  Using PollWatcher (checks file changes every 2s) for Codex sessions",
        ) {
            eprintln!("Logging error: {}", e);
        }

        let mut watcher = notify::PollWatcher::new(
            tx,
            Config::default()
                .with_poll_interval(FILE_WATCH_POLL_INTERVAL)
                .with_compare_contents(true), // Actually check file contents changed
        )?;

        // Discover and watch all existing session subdirectories (YYYY/MM/DD structure)
        let subdirs = Self::discover_session_subdirectories(&sessions_path)?;

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📂 Found {} Codex session subdirectories to watch",
                subdirs.len()
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }

        // Watch each subdirectory explicitly (more reliable than recursive on macOS)
        for subdir in &subdirs {
            watcher.watch(subdir, RecursiveMode::NonRecursive)?;
            if let Err(e) = log_info(
                PROVIDER_ID,
                &format!("📁 Watching Codex session directory: {}", subdir.display()),
            ) {
                eprintln!("Logging error: {}", e);
            }
        }

        // Also watch the root sessions directory for new subdirectories being created
        watcher.watch(&sessions_path, RecursiveMode::NonRecursive)?;
        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!("📂 Watching root sessions directory: {}", sessions_path.display()),
        ) {
            eprintln!("Logging error: {}", e);
        }

        let is_running = Arc::new(Mutex::new(true));
        let is_running_clone = Arc::clone(&is_running);
        let upload_queue_clone = Arc::clone(&upload_queue);
        let sessions_path_clone = sessions_path.clone();
        let event_bus_clone = event_bus.clone();

        // Start background thread to handle file events
        let thread_handle = thread::spawn(move || {
            Self::file_event_processor(
                rx,
                sessions_path_clone,
                upload_queue_clone,
                event_bus_clone,
                is_running_clone,
            );
        });

        Ok(CodexWatcher {
            _watcher: watcher,
            _thread_handle: thread_handle,
            upload_queue,
            is_running,
            event_bus,
        })
    }

    fn discover_session_subdirectories(
        sessions_path: &Path,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
        use std::fs;

        let mut subdirs = Vec::new();

        // Codex uses YYYY/MM/DD structure
        // Iterate through year directories
        if let Ok(entries) = fs::read_dir(sessions_path) {
            for entry in entries.flatten() {
                let year_path = entry.path();
                if year_path.is_dir() {
                    // Check if it looks like a year (4 digits)
                    if let Some(year_name) = year_path.file_name().and_then(|n| n.to_str()) {
                        if year_name.len() == 4 && year_name.chars().all(|c| c.is_ascii_digit()) {
                            // Iterate through month directories
                            if let Ok(month_entries) = fs::read_dir(&year_path) {
                                for month_entry in month_entries.flatten() {
                                    let month_path = month_entry.path();
                                    if month_path.is_dir() {
                                        // Check if it looks like a month (2 digits)
                                        if let Some(month_name) =
                                            month_path.file_name().and_then(|n| n.to_str())
                                        {
                                            if month_name.len() == 2
                                                && month_name.chars().all(|c| c.is_ascii_digit())
                                            {
                                                // Iterate through day directories
                                                if let Ok(day_entries) = fs::read_dir(&month_path) {
                                                    for day_entry in day_entries.flatten() {
                                                        let day_path = day_entry.path();
                                                        if day_path.is_dir() {
                                                            // Check if it looks like a day (2 digits)
                                                            if let Some(day_name) = day_path
                                                                .file_name()
                                                                .and_then(|n| n.to_str())
                                                            {
                                                                if day_name.len() == 2
                                                                    && day_name
                                                                        .chars()
                                                                        .all(|c| c.is_ascii_digit())
                                                                {
                                                                    subdirs.push(day_path);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(subdirs)
    }

    fn file_event_processor(
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
        sessions_path: PathBuf,
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
                        Self::process_file_event(&event, &sessions_path, &session_states)
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
                                    "🆕 New Codex session detected: {}",
                                    file_event.session_id
                                );
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Log session updates at info level
                                let log_message = format!(
                                    "📝 Codex session changed: {} (size: {} bytes)",
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
                    if let Err(e) = log_error(
                        PROVIDER_ID,
                        &format!("Codex file watcher error: {:?}", error),
                    ) {
                        eprintln!("Logging error: {}", e);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout is normal, continue watching
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Err(e) =
                        log_error(PROVIDER_ID, "Codex file watcher channel disconnected")
                    {
                        eprintln!("Logging error: {}", e);
                    }
                    break;
                }
            }
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Codex file monitoring stopped") {
            eprintln!("Logging error: {}", e);
        }
    }

    fn process_file_event(
        event: &Event,
        sessions_path: &Path,
        session_states: &SessionStateManager,
    ) -> Option<FileChangeEvent> {
        // Filter out event types we don't care about
        match &event.kind {
            EventKind::Access(_) | EventKind::Remove(_) | EventKind::Any | EventKind::Other => {
                return None;
            }
            _ => {}
        }

        // Only process write events for .jsonl files
        match &event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in &event.paths {
                    // Skip hidden files (starting with .)
                    if should_skip_file(path) {
                        continue;
                    }

                    // Check if it's a .jsonl file
                    if !has_extension(path, "jsonl") {
                        continue;
                    }

                    // Ensure it's within the sessions directory
                    if !path.starts_with(sessions_path) {
                        continue;
                    }

                    // Extract session ID from filename (always succeeds unless malformed)
                    if let Some(session_id) = Self::extract_session_id_from_filename(path) {
                        // Extract project name from file content (fallback to "unknown")
                        let project_name = Self::extract_project_name_from_file(path);

                        // Get file size
                        let file_size = get_file_size(path).unwrap_or(0);

                        return Some(FileChangeEvent {
                            path: path.clone(),
                            project_name,
                            file_size,
                            session_id: session_id.clone(),
                        });
                    } else {
                        // Only log if extraction truly failed
                        if let Err(e) = log_error(
                            PROVIDER_ID,
                            &format!("❌ Failed to extract session ID from filename: {}", path.display()),
                        ) {
                            eprintln!("Logging error: {}", e);
                        }
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn extract_session_id_from_filename(file_path: &Path) -> Option<String> {
        // Codex filename format: rollout-2025-10-06T22-15-35-{SESSION_ID}.jsonl
        // Session ID format: 8-4-4-4-12 hex digits (e.g., 0199bb2a-4c23-76b1-bfb0-2d78295c0f29)

        // First try to extract from filename
        if let Some(file_name) = file_path.file_stem().and_then(|s| s.to_str()) {
            // Look for the timestamp pattern (YYYY-MM-DDTHH-MM-SS-) and extract everything after it
            // The timestamp is always in the format: 2025-10-06T22-15-35-
            // Find the part after the last 'T' followed by time digits
            let parts: Vec<&str> = file_name.split('-').collect();

            // Find where the session ID starts (after the timestamp)
            // Session ID is 5 segments: 8-4-4-4-12 (36 chars total with dashes)
            if parts.len() >= 5 {
                // Try to find 5 consecutive segments that look like a UUID
                for i in 0..=parts.len().saturating_sub(5) {
                    let potential_uuid = format!(
                        "{}-{}-{}-{}-{}",
                        parts[i], parts[i + 1], parts[i + 2], parts[i + 3], parts[i + 4]
                    );
                    // Check if it looks like a valid UUID (36 chars, hex digits)
                    if potential_uuid.len() == 36
                        && potential_uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
                    {
                        return Some(potential_uuid);
                    }
                }
            }
        }

        // Fallback: Try to read session ID from JSON content
        Self::extract_session_id_from_json(file_path)
    }

    fn extract_session_id_from_json(file_path: &Path) -> Option<String> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        if let Ok(file) = File::open(file_path) {
            let reader = BufReader::new(file);
            // Check first few lines for session_meta entry
            for line in reader.lines().take(10) {
                if let Ok(line_content) = line {
                    if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line_content) {
                        // Check for sessionId field (Codex format)
                        if let Some(session_id) = entry.get("sessionId").and_then(|v| v.as_str()) {
                            return Some(session_id.to_string());
                        }
                        // Also check payload.id (alternative format)
                        if let Some(payload) = entry.get("payload") {
                            if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                                return Some(id.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Log error only if all extraction methods failed
        let _ = log_error(
            PROVIDER_ID,
            &format!("❌ Failed to extract session ID from: {}", file_path.display()),
        );

        None
    }

    fn extract_project_name_from_file(file_path: &Path) -> String {
        // Try to read first line to get project name from cwd
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        if let Ok(file) = File::open(file_path) {
            let reader = BufReader::new(file);
            // Check first few lines (in case session_meta isn't first)
            for line_result in reader.lines().take(10) {
                if let Ok(line_content) = line_result {
                    if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line_content) {
                        // Try to find CWD from various locations in the JSON
                        let cwd = entry.get("cwd").and_then(|v| v.as_str())
                            .or_else(|| entry.get("payload").and_then(|p| p.get("cwd")).and_then(|v| v.as_str()));

                        if let Some(cwd_path) = cwd {
                            return Path::new(cwd_path)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("unknown")
                                .to_string();
                        }
                    }
                }
            }
        }

        // Fallback to "unknown" if we can't read the file or find CWD
        "unknown".to_string()
    }

    pub fn stop(&self) {
        if let Ok(mut running) = self.is_running.lock() {
            *running = false;
        }

        if let Err(e) = log_info(PROVIDER_ID, "🛑 Stopping Codex file monitoring") {
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
pub type CodexWatcherStatus = WatcherStatus;

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
        let session_id = "test-session";
        let mut session_states = SessionStateManager::new();

        // Session not in states - should be considered new
        assert!(!session_states.contains(session_id));

        // Add session to states - should not be considered new
        session_states.get_or_create(session_id, 100);
        assert!(session_states.contains(session_id));
    }

    #[test]
    fn test_extract_session_id_from_filename() {
        let path = std::path::Path::new("/Users/user/.codex/sessions/2025/10/06/rollout-2025-10-06T22-15-35-0199bb2a-4c23-76b1-bfb0-2d78295c0f29.jsonl");
        let session_id = CodexWatcher::extract_session_id_from_filename(path);
        assert_eq!(session_id, Some("0199bb2a-4c23-76b1-bfb0-2d78295c0f29".to_string()));
    }
}
