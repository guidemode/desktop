use crate::config::load_provider_config;
use crate::logging::{log_debug, log_error, log_info};
use crate::providers::db_helpers::insert_session_immediately;
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, PollWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ID: &str = "codex";

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
    _watcher: PollWatcher,
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
                .with_poll_interval(Duration::from_secs(2))
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
                        Self::process_file_event(&event, &sessions_path, &session_states)
                    {
                        // Check if this is a new session or significant change (before updating state)
                        let should_log = Self::should_log_event(&file_event, &session_states);

                        // INSERT TO DATABASE IMMEDIATELY (no debounce)
                        if let Err(e) = insert_session_immediately(
                            PROVIDER_ID,
                            &file_event.project_name,
                            &file_event.session_id,
                            &file_event.path,
                            file_event.file_size,
                            None, // Hash will be calculated during upload
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
                                // First time this watcher session has seen this session
                                // (but it may already exist in database from previous run)
                                let log_message = format!(
                                    "🔍 Codex session detected: {} (size: {} bytes) → Processing",
                                    file_event.session_id, file_event.file_size
                                );
                                if let Err(e) = log_info(PROVIDER_ID, &log_message) {
                                    eprintln!("Logging error: {}", e);
                                }
                            } else {
                                // Use debug level for routine session activity
                                let log_message = format!(
                                    "📝 Codex session active: {} (size: {} bytes)",
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
        session_states: &HashMap<String, SessionState>,
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

                    // Extract session ID from filename (always succeeds unless malformed)
                    if let Some(session_id) = Self::extract_session_id_from_filename(path) {
                        // Extract project name from file content (fallback to "unknown")
                        let project_name = Self::extract_project_name_from_file(path);

                        // Get file size
                        let file_size = Self::get_file_size(path).unwrap_or(0);

                        let is_new = Self::is_new_session(&session_id, path, session_states);

                        return Some(FileChangeEvent {
                            path: path.clone(),
                            project_name,
                            last_modified: Instant::now(),
                            file_size,
                            session_id: session_id.clone(),
                            is_new_session: is_new,
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

    fn get_file_size(path: &Path) -> Result<u64, std::io::Error> {
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
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
                            // Check if cooldown has elapsed OR size changed significantly since last upload
                            let cooldown_elapsed =
                                file_event.last_modified.duration_since(last_uploaded_time)
                                    >= RE_UPLOAD_COOLDOWN;
                            let size_since_upload = file_event
                                .file_size
                                .saturating_sub(existing_state.last_uploaded_size);
                            let size_changed_significantly = size_since_upload >= MIN_SIZE_CHANGE_BYTES;

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
        let session_id = "test-session";
        let mut session_states = HashMap::new();
        let dummy_path = std::path::Path::new("dummy.jsonl");

        // Session not in states - should be considered new
        assert!(CodexWatcher::is_new_session(
            session_id,
            dummy_path,
            &session_states
        ));

        // Add session to states - should not be considered new
        session_states.insert(
            session_id.to_string(),
            SessionState {
                last_modified: Instant::now(),
                last_size: 100,
                is_active: true,
                upload_pending: false,
                last_uploaded_time: None,
                last_uploaded_size: 0,
            },
        );
        assert!(!CodexWatcher::is_new_session(
            session_id,
            dummy_path,
            &session_states
        ));
    }

    #[test]
    fn test_extract_session_id_from_filename() {
        let path = std::path::Path::new("/Users/user/.codex/sessions/2025/10/06/rollout-2025-10-06T22-15-35-0199bb2a-4c23-76b1-bfb0-2d78295c0f29.jsonl");
        let session_id = CodexWatcher::extract_session_id_from_filename(path);
        assert_eq!(session_id, Some("0199bb2a-4c23-76b1-bfb0-2d78295c0f29".to_string()));
    }
}
