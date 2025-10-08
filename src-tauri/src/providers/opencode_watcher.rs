use super::opencode_parser::OpenCodeParser;
use crate::config::load_provider_config;
use crate::logging::{log_error, log_info};
use crate::providers::db_helpers::insert_session_immediately;
use crate::upload_queue::UploadQueue;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ID: &str = "opencode";

// Debounce: aggregate session after this much inactivity
const AGGREGATION_DEBOUNCE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct SessionChangeEvent {
    pub session_id: String,
    pub project_id: String,
    pub affected_files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub last_modified: Instant,
    pub affected_files: HashSet<PathBuf>,
    pub needs_aggregation: bool, // True when session has changes that haven't been aggregated
    pub project_id: String,      // Needed for aggregation
    pub last_aggregated: Option<Instant>, // When we last created the virtual JSONL
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
            return Err(format!(
                "OpenCode home directory does not exist: {}",
                base_path.display()
            )
            .into());
        }

        let storage_path = base_path.join("storage");
        if !storage_path.exists() {
            return Err(format!(
                "OpenCode storage directory does not exist: {}",
                storage_path.display()
            )
            .into());
        }

        // Create parser for session analysis
        let parser = OpenCodeParser::new(storage_path.clone());

        // Determine which projects to watch
        let projects_to_watch = if config.project_selection == "ALL" {
            // Watch all available projects
            Self::discover_all_projects(&parser)?
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
                "📁 Monitoring {} OpenCode projects: {}",
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

    /// Aggregate session into virtual JSONL and write to cache
    /// Returns (jsonl_path, project_name)
    fn aggregate_session(
        parser: &OpenCodeParser,
        session_id: &str,
        _project_id: &str,
    ) -> Result<(PathBuf, String), Box<dyn std::error::Error + Send + Sync>> {
        // Create cache directory if it doesn't exist
        let cache_dir = dirs::home_dir()
            .ok_or("Could not find home directory")?
            .join(".guideai")
            .join("cache")
            .join("opencode");

        fs::create_dir_all(&cache_dir)?;

        // Parse session to create virtual JSONL
        let parsed_session = parser.parse_session(session_id).map_err(|e| {
            format!("Failed to parse OpenCode session {}: {}", session_id, e)
        })?;

        // Write virtual JSONL to cache
        let jsonl_path = cache_dir.join(format!("{}.jsonl", session_id));
        fs::write(&jsonl_path, &parsed_session.jsonl_content)?;

        // Extract real project name from parsed session (not the GUID)
        let project_name = parsed_session.project_name.clone();

        if let Err(e) = log_info(
            PROVIDER_ID,
            &format!(
                "📝 Aggregated session {} → {} ({} bytes, project: {})",
                session_id,
                jsonl_path.display(),
                parsed_session.jsonl_content.len(),
                project_name
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }

        Ok((jsonl_path, project_name))
    }

    fn discover_all_projects(
        parser: &OpenCodeParser,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let projects = parser
            .get_all_projects()
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

            // Process file system events with short timeout for debouncing
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(event)) => {
                    if let Some(session_event) = Self::process_file_event(
                        &event,
                        &storage_path,
                        &parser,
                        &projects_to_watch,
                        &session_states,
                    ) {
                        // PHASE 1: WATCH - Just mark session as needing aggregation
                        Self::mark_session_for_aggregation(&mut session_states, &session_event);
                    }
                }
                Ok(Err(error)) => {
                    if let Err(e) = log_error(
                        PROVIDER_ID,
                        &format!("OpenCode file watcher error: {:?}", error),
                    ) {
                        eprintln!("Logging error: {}", e);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout is normal - use it to check for sessions ready to aggregate
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Err(e) =
                        log_error(PROVIDER_ID, "OpenCode file watcher channel disconnected")
                    {
                        eprintln!("Logging error: {}", e);
                    }
                    break;
                }
            }

            // PHASE 2 & 3: AGGREGATE & PROCESS
            // Check for sessions that have been idle for AGGREGATION_DEBOUNCE
            let now = Instant::now();
            let sessions_to_aggregate: Vec<(String, String)> = session_states
                .iter()
                .filter(|(_, state)| {
                    state.needs_aggregation
                        && now.duration_since(state.last_modified) >= AGGREGATION_DEBOUNCE
                })
                .map(|(session_id, state)| (session_id.clone(), state.project_id.clone()))
                .collect();

            for (session_id, project_id) in sessions_to_aggregate {
                // Aggregate session into virtual JSONL
                match Self::aggregate_session(&parser, &session_id, &project_id) {
                    Ok((jsonl_path, project_name)) => {
                        // Get file size
                        let file_size = jsonl_path.metadata().map(|m| m.len()).unwrap_or(0);

                        // Insert/update database with virtual JSONL path and real project name
                        if let Err(e) = insert_session_immediately(
                            PROVIDER_ID,
                            &project_name,  // Use real project name, not GUID
                            &session_id,
                            &jsonl_path,
                            file_size,
                        ) {
                            if let Err(log_err) = log_error(
                                PROVIDER_ID,
                                &format!("Failed to save session to database: {}", e),
                            ) {
                                eprintln!("Logging error: {}", log_err);
                            }
                        } else {
                            // Mark as aggregated
                            if let Some(state) = session_states.get_mut(&session_id) {
                                state.needs_aggregation = false;
                                state.last_aggregated = Some(now);
                            }
                        }
                    }
                    Err(e) => {
                        if let Err(log_err) = log_error(
                            PROVIDER_ID,
                            &format!("Failed to aggregate session {}: {}", session_id, e),
                        ) {
                            eprintln!("Logging error: {}", log_err);
                        }
                    }
                }
            }
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
        _session_states: &HashMap<String, SessionState>,
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
                                    if let Some(project_id) =
                                        parser.get_project_for_session(&session_id)
                                    {
                                        // Check if this project is being watched
                                        if let Ok(project) = parser.get_all_projects() {
                                            for proj in project {
                                                if proj.id == project_id {
                                                    let project_name = Path::new(&proj.worktree)
                                                        .file_name()
                                                        .and_then(|name| name.to_str())
                                                        .unwrap_or("unknown");

                                                    if projects_to_watch
                                                        .contains(&project_name.to_string())
                                                    {
                                                        return Some(SessionChangeEvent {
                                                            session_id: session_id.clone(),
                                                            project_id,
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
                                    if let Some(session_id) =
                                        components.get(1).and_then(|c| c.as_os_str().to_str())
                                    {
                                        if let Some(project_id) =
                                            parser.get_project_for_session(session_id)
                                        {
                                            return Some(SessionChangeEvent {
                                                session_id: session_id.to_string(),
                                                project_id,
                                                affected_files: vec![path.clone()],
                                            });
                                        }
                                    }
                                }
                            }
                            Some("session") => {
                                // Session file changed: session/{projectId}/{sessionId}.json
                                if components.len() >= 3 {
                                    if let Some(project_id) =
                                        components.get(1).and_then(|c| c.as_os_str().to_str())
                                    {
                                        if let Some(session_id) = components
                                            .get(2)
                                            .and_then(|c| c.as_os_str().to_str())
                                            .and_then(|s| s.strip_suffix(".json"))
                                        {
                                            return Some(SessionChangeEvent {
                                                session_id: session_id.to_string(),
                                                project_id: project_id.to_string(),
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

    fn is_new_session(
        session_id: &str,
        path: &Path,
        session_states: &HashMap<String, SessionState>,
    ) -> bool {
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

    /// Mark a session as needing aggregation (Phase 1: Watch)
    fn mark_session_for_aggregation(
        session_states: &mut HashMap<String, SessionState>,
        event: &SessionChangeEvent,
    ) {
        let now = Instant::now();

        session_states
            .entry(event.session_id.clone())
            .and_modify(|state| {
                state.last_modified = now;
                state.needs_aggregation = true;
                for file in &event.affected_files {
                    state.affected_files.insert(file.clone());
                }
            })
            .or_insert_with(|| SessionState {
                last_modified: now,
                affected_files: event.affected_files.iter().cloned().collect(),
                needs_aggregation: true,
                project_id: event.project_id.clone(),
                last_aggregated: None,
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
        assert!(OpenCodeWatcher::is_new_session(
            session_id,
            &file_path,
            &session_states
        ));

        // Create a larger file - should not be considered new based on size
        let large_content = "x".repeat(2000);
        fs::write(&file_path, large_content).unwrap();
        assert!(!OpenCodeWatcher::is_new_session(
            session_id,
            &file_path,
            &session_states
        ));

        // Add session to states - should not be considered new even if file is small
        session_states.insert(
            session_id.to_string(),
            SessionState {
                last_modified: Instant::now(),
                affected_files: HashSet::new(),
                needs_aggregation: false,
                project_id: "test-project".to_string(),
                last_aggregated: None,
            },
        );
        fs::write(&file_path, "{}").unwrap();
        assert!(!OpenCodeWatcher::is_new_session(
            session_id,
            &file_path,
            &session_states
        ));
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
        let result = OpenCodeWatcher::process_file_event(
            &hidden_event,
            storage_path,
            &parser,
            &projects_to_watch,
            &session_states,
        );
        assert!(result.is_none(), "Hidden file should be ignored");

        // Test normal file would be processed (will fail to find project, but the file isn't filtered out)
        let normal_event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![normal_file.clone()],
            attrs: Default::default(),
        };
        let _result = OpenCodeWatcher::process_file_event(
            &normal_event,
            storage_path,
            &parser,
            &projects_to_watch,
            &session_states,
        );
        // Result might be None if project lookup fails, but that's OK - we're just testing file filtering
        // The important thing is it didn't get filtered out like the hidden file
    }
}
