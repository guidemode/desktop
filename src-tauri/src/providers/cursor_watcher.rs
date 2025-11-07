/// Cursor provider watcher with hybrid directory + database polling
///
/// Architecture:
/// - Scans existing sessions on startup
/// - Watches ~/.cursor/chats for new session directories
/// - Polls active sessions (from our database) using PRAGMA data_version
/// - Only polls sessions updated in last hour (automatic pruning)
use crate::config::load_provider_config;
use crate::database::with_connection_mut;
use crate::events::{EventBus, SessionEventPayload};
use crate::providers::cursor::{db, discover_sessions, get_db_path_for_session, scan_existing_sessions};
use crate::providers::common::get_canonical_path;
use crate::upload_queue::UploadQueue;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

const PROVIDER_ID: &str = "cursor";
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const ACTIVE_WINDOW_HOURS: i64 = 1;

/// Session tracker for database polling
struct SessionTracker {
    #[allow(dead_code)] // Used as HashMap key, not accessed directly
    session_id: String,
    db_path: PathBuf,
    last_data_version: i64,
    last_checked: SystemTime,
}

#[derive(Debug)]
pub struct CursorWatcher {
    _watcher: RecommendedWatcher,
    _poll_thread: thread::JoinHandle<()>,
    is_running: Arc<Mutex<bool>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[derive(Default)]
pub struct CursorWatcherStatus {
    pub is_running: bool,
    pub active_sessions: usize,
}


impl CursorWatcher {
    pub fn new(
        _projects: Vec<String>,
        upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("🔍 Starting Cursor session monitoring");

        // Load provider config
        let config = load_provider_config(PROVIDER_ID)
            .map_err(|e| format!("Failed to load provider config: {}", e))?;

        if !config.enabled {
            return Err("Cursor provider is not enabled".into());
        }

        // Part 1: Run initial scan of existing sessions
        tracing::info!("📊 Scanning existing Cursor sessions...");
        match scan_existing_sessions(&event_bus) {
            Ok(result) => {
                tracing::info!(
                    "✨ Initial scan complete: {} sessions, {} messages",
                    result.sessions_processed,
                    result.messages_converted
                );
            }
            Err(e) => {
                tracing::warn!("⚠️  Initial scan encountered errors: {:?}", e);
            }
        }

        // Part 2: Setup directory watcher for new sessions
        let chats_path = shellexpand::tilde("~/.cursor/chats").to_string();
        let chats_dir = std::path::Path::new(&chats_path);

        if !chats_dir.exists() {
            return Err(format!("Cursor chats directory not found: {}", chats_path).into());
        }

        tracing::info!("📁 Watching Cursor chats directory: {}", chats_path);

        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            tx,
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        // Watch the chats directory recursively for new sessions
        watcher.watch(chats_dir, RecursiveMode::Recursive)?;

        // Part 3: Start hybrid event loop (filesystem + database polling)
        let is_running = Arc::new(Mutex::new(true));
        let is_running_clone = is_running.clone();
        let upload_queue_clone = upload_queue.clone();
        let event_bus_clone = event_bus.clone();

        let poll_thread = thread::spawn(move || {
            Self::hybrid_event_loop(rx, is_running_clone, upload_queue_clone, event_bus_clone);
        });

        Ok(CursorWatcher {
            _watcher: watcher,
            _poll_thread: poll_thread,
            is_running,
        })
    }

    /// Hybrid event loop: handles both filesystem events and database polling
    fn hybrid_event_loop(
        rx: mpsc::Receiver<notify::Result<notify::Event>>,
        is_running: Arc<Mutex<bool>>,
        _upload_queue: Arc<UploadQueue>,
        event_bus: EventBus,
    ) {
        let mut session_trackers: HashMap<String, SessionTracker> = HashMap::new();
        let mut last_poll = SystemTime::now();

        loop {
            // Check shutdown
            if !*is_running.lock().unwrap() {
                tracing::info!("🛑 Cursor watcher shutting down");
                break;
            }

            // Part 1: Handle filesystem events (new session detection)
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    if let Some(session_id) = Self::handle_filesystem_event(event) {
                        tracing::info!("🆕 New Cursor session detected: {}", session_id);

                        // Try to process immediately
                        match Self::process_new_session(&session_id, &event_bus) {
                            Ok(()) => {
                                tracing::debug!("✅ Processed new session: {}", session_id);
                            }
                            Err(e) => {
                                tracing::warn!("❌ Failed to process new session {}: {:?}", session_id, e);
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Filesystem watch error: {:?}", e);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // No filesystem events, continue to polling
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::error!("Filesystem watch channel disconnected");
                    break;
                }
            }

            // Part 2: Smart polling (only active sessions from our database)
            if last_poll.elapsed().unwrap_or(Duration::ZERO) >= POLL_INTERVAL {
                Self::poll_active_sessions(&mut session_trackers, &event_bus);
                last_poll = SystemTime::now();
            }
        }
    }

    /// Handle filesystem events to detect new Cursor sessions
    fn handle_filesystem_event(event: notify::Event) -> Option<String> {
        for path in event.paths {
            // Check if this is a new store.db file
            if path.file_name()?.to_str()? == "store.db" {
                // Extract session ID from parent directory
                let session_dir = path.parent()?;
                let session_id = session_dir.file_name()?.to_str()?.to_string();

                // Validate it looks like a UUID
                if session_id.contains('-') && session_id.len() == 36 {
                    return Some(session_id);
                }
            }
        }
        None
    }

    /// Process a newly detected session
    fn process_new_session(
        session_id: &str,
        event_bus: &EventBus,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Re-discover sessions to find the new one
        let sessions = discover_sessions()?;

        let session = sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .ok_or_else(|| format!("Session {} not found after discovery", session_id))?;

        // Use scanner logic to process single session
        use crate::providers::cursor::converter::CursorMessageWithRaw;
        use crate::providers::cursor::scanner;
        

        let conn = db::open_cursor_db(&session.db_path)?;
        let decoded_messages = db::get_decoded_messages(&conn)?;

        let mut canonical_messages = Vec::new();
        for (message_index, (_msg_id, raw_data, msg)) in decoded_messages.iter().enumerate() {
            // Wrap message with raw data and session metadata for timestamp calculation
            let msg_with_raw = CursorMessageWithRaw::new(
                msg,
                raw_data,
                session.metadata.created_at,
                message_index,
            );

            // Use split conversion to prevent UUID collisions
            if let Ok(messages) = msg_with_raw.to_canonical_split() {
                for mut canonical in messages {
                    canonical.session_id = session.session_id.clone();
                    if canonical.cwd.is_none() {
                        canonical.cwd = session.cwd.clone();
                    }
                    canonical_messages.push(canonical);
                }
            }
        }

        if canonical_messages.is_empty() {
            return Ok(()); // No messages yet, skip
        }

        // Get canonical path and write (use CWD if available)
        let canonical_path = get_canonical_path(
            PROVIDER_ID,
            session.cwd.as_deref(),
            &session.session_id,
        )
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(std::io::Error::other(e.to_string())) })?;

        scanner::write_canonical_file(&canonical_path, &canonical_messages)?;

        // Get file size and publish event
        let file_size = std::fs::metadata(&canonical_path)?.len();

        event_bus.publish(
            PROVIDER_ID,
            SessionEventPayload::SessionChanged {
                session_id: session.session_id.clone(),
                project_name: session.project_name(),
                file_path: canonical_path,
                file_size,
            },
        )?;

        Ok(())
    }

    /// Poll active sessions from our database
    fn poll_active_sessions(
        session_trackers: &mut HashMap<String, SessionTracker>,
        event_bus: &EventBus,
    ) {
        // Query OUR database for recently active Cursor sessions
        let active_sessions = match Self::get_active_sessions_from_db() {
            Ok(sessions) => sessions,
            Err(e) => {
                tracing::error!("Failed to query active sessions: {:?}", e);
                return;
            }
        };

        tracing::debug!("🔄 Polling {} active Cursor sessions", active_sessions.len());

        // Update tracker list (remove sessions no longer active)
        let active_ids: std::collections::HashSet<String> =
            active_sessions.iter().map(|s| s.0.clone()).collect();

        session_trackers.retain(|id, _| active_ids.contains(id));

        // Poll each active session
        for (session_id, canonical_path) in active_sessions {
            // Get or create tracker
            let tracker = session_trackers.entry(session_id.clone()).or_insert_with(|| {
                // Try to get DB path for this session
                let db_path = get_db_path_for_session(&session_id).unwrap_or_default();

                // Phase 1 Fix: Initialize with current data_version to prevent false positives
                let initial_version = if db_path.exists() {
                    db::open_cursor_db(&db_path)
                        .and_then(|conn| db::get_data_version(&conn))
                        .unwrap_or(0)
                } else {
                    0
                };

                SessionTracker {
                    session_id: session_id.clone(),
                    db_path,
                    last_data_version: initial_version,
                    last_checked: SystemTime::now(),
                }
            });

            // Check for changes using PRAGMA data_version
            match Self::check_session_changed(tracker) {
                Ok(true) => {
                    // Phase 3 Enhancement: Verify content actually changed before reprocessing
                    // Prevents redundant processing when only SQLite metadata changed
                    let should_reprocess = match Self::verify_content_changed(&canonical_path, &tracker.db_path) {
                        Ok(changed) => changed,
                        Err(e) => {
                            tracing::debug!("Could not verify content change for {}: {:?}, will reprocess", session_id, e);
                            true // Default to reprocessing if verification fails
                        }
                    };

                    if should_reprocess {
                        tracing::info!("🔄 Session {} has content changes, reprocessing", session_id);

                        // Reprocess session
                        if let Err(e) = Self::process_new_session(&session_id, event_bus) {
                            tracing::warn!("Failed to reprocess session {}: {:?}", session_id, e);
                        }
                    } else {
                        tracing::debug!("Session {} data_version changed but content unchanged, skipping", session_id);
                    }
                }
                Ok(false) => {
                    // No changes
                }
                Err(e) => {
                    tracing::warn!("Failed to check session {}: {:?}", session_id, e);
                }
            }

            tracker.last_checked = SystemTime::now();
        }
    }

    /// Query our database for recently active Cursor sessions
    fn get_active_sessions_from_db() -> Result<Vec<(String, String)>, rusqlite::Error> {
        with_connection_mut(|conn| {
            // Phase 2 Optimization: Only poll sessions created/started in last hour
            // Reduces overhead by ~80-90% for users with many old sessions
            // Use session_start_time if available, otherwise fall back to created_at
            let cutoff_timestamp = (SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64)
                - (ACTIVE_WINDOW_HOURS * 3600);

            let mut stmt = conn.prepare(
                "SELECT session_id, file_path
                 FROM agent_sessions
                 WHERE provider = 'cursor'
                 AND session_end_time IS NULL
                 AND (session_start_time >= ? OR (session_start_time IS NULL AND created_at >= ?))
                 ORDER BY COALESCE(session_start_time, created_at) DESC
                 LIMIT 50",
            )?;

            let sessions = stmt.query_map([cutoff_timestamp, cutoff_timestamp], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

            Ok(sessions)
        })
    }

    /// Check if a session's database has changed using PRAGMA data_version
    fn check_session_changed(tracker: &mut SessionTracker) -> Result<bool, Box<dyn std::error::Error>> {
        if !tracker.db_path.exists() {
            return Ok(false);
        }

        let conn = db::open_cursor_db(&tracker.db_path)?;
        let current_version = db::get_data_version(&conn)?;

        if current_version != tracker.last_data_version {
            tracker.last_data_version = current_version;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Verify content actually changed by comparing message count
    /// Returns true if content changed, false if unchanged
    fn verify_content_changed(
        canonical_path: &str,
        db_path: &Path,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        // If canonical file doesn't exist, definitely need to create it
        if !std::path::Path::new(canonical_path).exists() {
            return Ok(true);
        }

        // Count messages in Cursor database
        let conn = db::open_cursor_db(db_path)?;
        let db_message_count = db::get_decoded_messages(&conn)?.len();

        // Count lines in canonical JSONL file (each line = 1 message)
        let canonical_message_count = std::fs::read_to_string(canonical_path)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();

        // If counts differ, content changed
        Ok(db_message_count != canonical_message_count)
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut is_running = self.is_running.lock().map_err(|e| e.to_string())?;
        *is_running = false;
        Ok(())
    }

    pub fn get_status(&self) -> Result<CursorWatcherStatus, String> {
        let is_running = *self.is_running.lock().map_err(|e| e.to_string())?;

        // Query active sessions count
        let active_sessions = Self::get_active_sessions_from_db()
            .map(|sessions| sessions.len())
            .unwrap_or(0);

        Ok(CursorWatcherStatus {
            is_running,
            active_sessions,
        })
    }
}

impl Drop for CursorWatcher {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            tracing::error!("Error stopping Cursor watcher: {}", e);
        }
    }
}
