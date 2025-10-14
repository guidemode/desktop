# Rust Codebase Improvements Plan

**Branch:** `rust-improvements`
**Created:** 2025-10-14
**Status:** Planning Phase

---

## Executive Summary

This document outlines a comprehensive plan to improve the Rust desktop application codebase across four phases:
1. **Critical Fixes** - Race conditions and data integrity issues
2. **Reduce Duplication** - Extract common provider watcher code
3. **Event-Driven Architecture** - Decouple watchers from database
4. **Type Safety & Polish** - Improve type safety and error handling

**Estimated Total Effort:** 10-15 days
**Lines of Code Impact:** ~500 lines removed, ~300 lines refactored, ~400 lines added
**Risk Level:** Medium (requires careful testing of concurrent operations)

---

## Phase 1: Critical Fixes (Priority: HIGH, 1-2 days)

### 1.1 Fix Database Race Conditions

**Location:** `apps/desktop/src-tauri/src/database.rs`

#### Issue 1: `update_session()` Race Condition (lines 169-299)
```rust
// CURRENT CODE (RACE CONDITION):
let (existing_start, existing_cwd, ...) = conn.query_row(...)?; // Read
// <- Another thread could update here
conn.execute("UPDATE ...", ...)?; // Write
```

**Problem:** Between the read and write, another thread could update the same session, causing lost updates.

**Solution:** Use SQLite transaction to make read-modify-write atomic:
```rust
pub fn update_session(...) -> Result<()> {
    with_connection_mut(|conn| {
        let tx = conn.transaction()?;

        // All reads and writes within transaction
        let (existing_start, existing_cwd, ...) = tx.query_row(...)?;
        tx.execute("UPDATE ...", ...)?;

        tx.commit()?;
        Ok(())
    })
}
```

**Files to modify:**
- `src/database.rs` - Add transaction wrapper to `update_session()`
- Add tests for concurrent updates

**Success criteria:**
- ✅ Concurrent updates don't lose data
- ✅ All existing tests pass
- ✅ New concurrent update test passes

---

#### Issue 2: `insert_session_immediately()` Check-Then-Act Race (db_helpers.rs:33-100)
```rust
// CURRENT CODE (RACE CONDITION):
if session_exists(session_id, file_name)? {  // Check
    update_session(...)?;  // Update
} else {
    insert_session(...)?;  // Insert  <- RACE: both threads might insert
}
```

**Problem:** Two threads can both check, both see no session, both try to insert → UNIQUE constraint violation.

**Solution Option A (Simpler):** Use `INSERT OR REPLACE`:
```rust
pub fn insert_session_immediately(...) -> Result<()> {
    // Use INSERT OR REPLACE for atomicity
    conn.execute(
        "INSERT OR REPLACE INTO agent_sessions (...) VALUES (...)",
        params![...]
    )?;
    Ok(())
}
```

**Solution Option B (More Control):** Explicit transaction with error handling:
```rust
pub fn insert_session_immediately(...) -> Result<()> {
    with_connection_mut(|conn| {
        let tx = conn.transaction()?;

        match insert_session_in_tx(&tx, ...) {
            Ok(id) => {
                tx.commit()?;
                Ok(id)
            }
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation => {
                // Already exists, update instead
                update_session_in_tx(&tx, ...)?;
                tx.commit()?;
                Ok(existing_id)
            }
            Err(e) => Err(e)
        }
    })
}
```

**Recommendation:** Use Option A initially for simplicity, can add Option B if more control needed.

**Files to modify:**
- `src/providers/db_helpers.rs` - Refactor `insert_session_immediately()`
- Add concurrent insert tests

**Success criteria:**
- ✅ Concurrent inserts for same session don't fail
- ✅ No duplicate sessions in database
- ✅ Last write wins for session data

---

#### Issue 3: Add Optimistic Locking (Optional but Recommended)

**Problem:** Even with transactions, we can't detect when another process updated the same session.

**Solution:** Add version column to detect concurrent modifications:
```sql
-- Migration 017
ALTER TABLE agent_sessions ADD COLUMN version INTEGER DEFAULT 1;
```

```rust
pub fn update_session_with_version(...) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE agent_sessions
         SET file_size = ?, version = version + 1, ...
         WHERE session_id = ? AND version = ?",
        params![file_size, session_id, expected_version]
    )?;

    Ok(rows > 0) // Returns false if version mismatch
}
```

**Files to modify:**
- `src-tauri/migrations/017_add_version_column.sql` (new file)
- `src/database.rs` - Add version parameter to update functions
- Optional: Add retry logic when version conflicts

**Success criteria:**
- ✅ Version conflicts are detected
- ✅ Application handles version conflicts gracefully
- ✅ Metrics on conflict rate are low

---

### 1.2 Improve Error Handling

**Problem:** Inconsistent error types (`String`, `Box<dyn Error>`, `rusqlite::Error`) make it hard to handle errors properly.

**Solution:** Create custom error types:
```rust
// src/error.rs (expand existing)
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Concurrent modification detected")]
    ConcurrentModification,

    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

**Files to modify:**
- `src/error.rs` - Add `DatabaseError` enum
- `src/database.rs` - Use `DatabaseError` instead of `Result<T, rusqlite::Error>`
- `src/providers/db_helpers.rs` - Propagate `DatabaseError`

**Success criteria:**
- ✅ Consistent error types across database operations
- ✅ Better error messages for debugging
- ✅ Easier to add error handling in callers

---

## Phase 2: Reduce Duplication (Priority: MEDIUM, 2-3 days)

### 2.1 Extract Common Watcher Utilities

**Problem:** 5 provider watchers have ~400-500 lines of duplicated code.

**Current Duplication Analysis:**
```
├── FileChangeEvent struct (5 copies, 95% identical)
├── SessionState struct (5 copies, 100% identical)
├── WatcherStatus struct (5 copies, 100% identical)
├── Constants (RE_UPLOAD_COOLDOWN, MIN_SIZE_CHANGE_BYTES) (5 copies)
├── should_log_event() function (5 copies, 95% identical)
├── update_session_state() function (5 copies, 90% identical)
├── is_new_session() function (5 copies, 80% identical)
├── get_status() implementation (5 copies, 100% identical)
└── Hidden file filtering (5 copies, 100% identical)
```

**Solution:** Create shared module structure:
```
src/providers/
├── common/
│   ├── mod.rs              # Public exports
│   ├── session_state.rs    # SessionState + update logic
│   ├── file_utils.rs       # File filtering, sizing
│   ├── watcher_status.rs   # Generic status types
│   ├── watcher_base.rs     # Base trait + common logic
│   └── constants.rs        # Shared constants
├── claude.rs               # Reduced to ~250 lines
├── claude_watcher.rs       # Reduced to ~350 lines
├── copilot.rs
├── copilot_watcher.rs
...
```

---

### 2.2 Create `SessionState` Module

**File:** `src/providers/common/session_state.rs`

```rust
use std::time::Instant;
use std::collections::HashMap;

/// Tracks state for a single session across file changes
#[derive(Debug, Clone)]
pub struct SessionState {
    pub last_modified: Instant,
    pub last_size: u64,
    pub is_active: bool,
    pub upload_pending: bool,
    pub last_uploaded_time: Option<Instant>,
    pub last_uploaded_size: u64,
}

impl SessionState {
    pub fn new(file_size: u64) -> Self {
        Self {
            last_modified: Instant::now(),
            last_size: file_size,
            is_active: true,
            upload_pending: false,
            last_uploaded_time: None,
            last_uploaded_size: 0,
        }
    }

    /// Update state with new file change event
    pub fn update(&mut self, file_size: u64, re_upload_cooldown: Duration, min_size_change: u64) {
        self.last_modified = Instant::now();
        self.last_size = file_size;
        self.is_active = true;

        // Smart re-upload logic
        if self.upload_pending {
            let should_allow_reupload = if let Some(last_uploaded) = self.last_uploaded_time {
                let cooldown_elapsed = self.last_modified.duration_since(last_uploaded) >= re_upload_cooldown;
                let size_changed = file_size.saturating_sub(self.last_uploaded_size) >= min_size_change;
                cooldown_elapsed || size_changed
            } else {
                true
            };

            if should_allow_reupload {
                self.upload_pending = false;
            }
        }
    }

    /// Should we log this change?
    pub fn should_log(&self, new_size: u64, min_size_change: u64, is_new: bool) -> bool {
        is_new || new_size.saturating_sub(self.last_size) >= min_size_change
    }
}

/// Manager for tracking multiple session states
pub struct SessionStateManager {
    states: HashMap<String, SessionState>,
}

impl SessionStateManager {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    pub fn get_or_create(&mut self, session_id: &str, file_size: u64) -> &mut SessionState {
        self.states
            .entry(session_id.to_string())
            .or_insert_with(|| SessionState::new(file_size))
    }

    pub fn contains(&self, session_id: &str) -> bool {
        self.states.contains_key(session_id)
    }
}
```

**Files to create:**
- `src/providers/common/session_state.rs`
- `src/providers/common/session_state_tests.rs`

**Files to modify:**
- All 5 watcher files to use shared `SessionState`
- Remove duplicate `SessionState` and `update_session_state()` functions

**Success criteria:**
- ✅ All watchers use shared `SessionState`
- ✅ ~100 lines of code removed per watcher (500 total)
- ✅ All existing tests pass

---

### 2.3 Create File Utilities Module

**File:** `src/providers/common/file_utils.rs`

```rust
use std::path::Path;

/// Check if a file should be filtered out (hidden files, temp files)
pub fn should_skip_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

/// Get file size safely
pub fn get_file_size(path: &Path) -> Result<u64, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    Ok(metadata.len())
}

/// Check if file matches extension
pub fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == ext)
        .unwrap_or(false)
}

/// Extract session ID from filename (various patterns)
pub fn extract_session_id_from_filename(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_file() {
        assert!(should_skip_file(Path::new(".hidden")));
        assert!(should_skip_file(Path::new("/path/.hidden")));
        assert!(!should_skip_file(Path::new("visible.txt")));
    }

    #[test]
    fn test_has_extension() {
        assert!(has_extension(Path::new("file.json"), "json"));
        assert!(!has_extension(Path::new("file.txt"), "json"));
        assert!(!has_extension(Path::new("file"), "json"));
    }
}
```

**Files to create:**
- `src/providers/common/file_utils.rs`

**Files to modify:**
- All watcher files to use shared utilities
- Remove duplicate file filtering logic

**Success criteria:**
- ✅ Consistent file filtering across all providers
- ✅ ~50 lines of code removed per watcher (250 total)
- ✅ Unit tests for all utilities

---

### 2.4 Create Generic Watcher Status

**File:** `src/providers/common/watcher_status.rs`

```rust
use serde::{Deserialize, Serialize};

/// Generic watcher status that all providers share
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherStatus {
    pub is_running: bool,
    pub pending_uploads: usize,
    pub processing_uploads: usize,
    pub failed_uploads: usize,
}

impl WatcherStatus {
    pub fn stopped() -> Self {
        Self {
            is_running: false,
            pending_uploads: 0,
            processing_uploads: 0,
            failed_uploads: 0,
        }
    }
}
```

**Files to create:**
- `src/providers/common/watcher_status.rs`

**Files to modify:**
- Remove `ClaudeWatcherStatus`, `CopilotWatcherStatus`, etc.
- Update type aliases: `pub type ClaudeWatcherStatus = WatcherStatus;`
- Update all `get_status()` implementations to return `WatcherStatus`

**Success criteria:**
- ✅ Single WatcherStatus type for all providers
- ✅ ~25 lines of code removed per watcher (125 total)
- ✅ No breaking changes to API

---

### 2.5 Create Constants Module

**File:** `src/providers/common/constants.rs`

```rust
use std::time::Duration;

// Cooldown timers
#[cfg(debug_assertions)]
pub const RE_UPLOAD_COOLDOWN: Duration = Duration::from_secs(30);

#[cfg(not(debug_assertions))]
pub const RE_UPLOAD_COOLDOWN: Duration = Duration::from_secs(300);

// Size thresholds
pub const MIN_SIZE_CHANGE_BYTES: u64 = 1024; // 1KB

// Polling intervals
pub const FILE_WATCH_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub const EVENT_TIMEOUT: Duration = Duration::from_secs(5);
```

**Files to create:**
- `src/providers/common/constants.rs`

**Files to modify:**
- All watcher files to use shared constants
- Remove duplicate constant definitions

**Success criteria:**
- ✅ Single source of truth for timing constants
- ✅ Easy to adjust timing across all providers
- ✅ ~10 lines removed per watcher (50 total)

---

## Phase 3: Event-Driven Architecture (Priority: MEDIUM, 3-5 days)

### 3.1 Design Event Types

**Problem:** Watchers are tightly coupled to database implementation. Changes to schema require updating all watchers.

**Current Flow:**
```
Watcher → db_helpers::insert_session_immediately() → Database
                                                    ↓
                                              APP_HANDLE.emit()
```

**Proposed Flow:**
```
Watcher → SessionEvent → EventBus → [
    DatabaseHandler,
    UploadHandler,
    MetricsHandler,
    FrontendEmitter
]
```

**Benefits:**
- ✅ Decouple watchers from database schema
- ✅ Transactional event processing
- ✅ Easy to add new event handlers
- ✅ Event replay for debugging
- ✅ Better testability

---

### 3.2 Create Event Types

**File:** `src/events/types.rs`

```rust
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Sequence number for ordering events
pub type EventSequence = u64;

/// All session-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub sequence: EventSequence,
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub payload: SessionEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventPayload {
    /// New session detected
    Created {
        session_id: String,
        project_name: String,
        file_path: PathBuf,
        file_size: u64,
        cwd: Option<String>,
    },

    /// Existing session updated
    Updated {
        session_id: String,
        file_size: u64,
        file_hash: Option<String>,
        cwd: Option<String>,
    },

    /// Session completed (has end time)
    Completed {
        session_id: String,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        duration_ms: i64,
    },

    /// Session processing failed
    Failed {
        session_id: String,
        reason: String,
    },
}

impl SessionEvent {
    pub fn session_id(&self) -> &str {
        match &self.payload {
            SessionEventPayload::Created { session_id, .. } => session_id,
            SessionEventPayload::Updated { session_id, .. } => session_id,
            SessionEventPayload::Completed { session_id, .. } => session_id,
            SessionEventPayload::Failed { session_id, .. } => session_id,
        }
    }
}
```

**Files to create:**
- `src/events/mod.rs`
- `src/events/types.rs`
- `src/events/bus.rs` (next section)

---

### 3.3 Implement Event Bus

**File:** `src/events/bus.rs`

```rust
use tokio::sync::broadcast;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub type EventReceiver = broadcast::Receiver<SessionEvent>;
pub type EventSender = broadcast::Sender<SessionEvent>;

/// Event bus for distributing session events
pub struct EventBus {
    sender: EventSender,
    sequence: Arc<AtomicU64>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            sequence: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Publish an event (returns sequence number)
    pub fn publish(&self, provider: &str, payload: SessionEventPayload) -> Result<EventSequence, String> {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);

        let event = SessionEvent {
            sequence,
            timestamp: Utc::now(),
            provider: provider.to_string(),
            payload,
        };

        self.sender.send(event)
            .map(|_| sequence)
            .map_err(|e| format!("Failed to publish event: {}", e))
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> EventReceiver {
        self.sender.subscribe()
    }

    /// Get current sequence number
    pub fn current_sequence(&self) -> EventSequence {
        self.sequence.load(Ordering::SeqCst)
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            sequence: Arc::clone(&self.sequence),
        }
    }
}
```

**Files to create:**
- `src/events/bus.rs`
- `src/events/handlers.rs` (next section)

---

### 3.4 Create Event Handlers

**File:** `src/events/handlers.rs`

```rust
use super::{EventBus, SessionEvent, SessionEventPayload};
use crate::database;
use crate::logging::{log_info, log_error};
use tokio::task;

/// Handler that writes events to database
pub struct DatabaseEventHandler {
    event_bus: EventBus,
}

impl DatabaseEventHandler {
    pub fn new(event_bus: EventBus) -> Self {
        Self { event_bus }
    }

    pub fn start(self) -> task::JoinHandle<()> {
        task::spawn(async move {
            let mut rx = self.event_bus.subscribe();

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Err(e) = self.handle_event(&event) {
                            log_error(&event.provider, &format!("Database handler error: {}", e))
                                .unwrap_or_default();
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        log_info("events", "Database handler stopped").unwrap_or_default();
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log_error("events", &format!("Database handler lagged {} events", n))
                            .unwrap_or_default();
                    }
                }
            }
        })
    }

    fn handle_event(&self, event: &SessionEvent) -> Result<(), String> {
        match &event.payload {
            SessionEventPayload::Created { session_id, project_name, file_path, file_size, cwd } => {
                database::insert_session(
                    &event.provider,
                    project_name,
                    session_id,
                    &file_path.to_string_lossy(),
                    &file_path.to_string_lossy(),
                    *file_size,
                    None, // hash
                    None, // start time
                    None, // end time
                    None, // duration
                    cwd.as_deref(),
                    None, // git_branch
                    None, // first_commit
                    None, // latest_commit
                ).map_err(|e| e.to_string())?;
            }

            SessionEventPayload::Updated { session_id, file_size, file_hash, cwd } => {
                database::update_session(
                    session_id,
                    "", // file_name (not used in query)
                    *file_size,
                    file_hash.as_deref(),
                    None, // start time
                    None, // end time
                    cwd.as_deref(),
                    None, // git_branch
                    None, // latest_commit
                ).map_err(|e| e.to_string())?;
            }

            SessionEventPayload::Completed { session_id, start_time, end_time, duration_ms } => {
                // Update with timing information
                database::update_session(
                    session_id,
                    "",
                    0, // file_size not changed
                    None,
                    Some(*start_time),
                    Some(*end_time),
                    None,
                    None,
                    None,
                ).map_err(|e| e.to_string())?;
            }

            SessionEventPayload::Failed { session_id, reason } => {
                database::mark_session_sync_failed(session_id, reason)
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }
}

/// Handler that emits events to frontend
pub struct FrontendEventHandler {
    event_bus: EventBus,
    app_handle: tauri::AppHandle,
}

impl FrontendEventHandler {
    pub fn new(event_bus: EventBus, app_handle: tauri::AppHandle) -> Self {
        Self { event_bus, app_handle }
    }

    pub fn start(self) -> task::JoinHandle<()> {
        task::spawn(async move {
            let mut rx = self.event_bus.subscribe();

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        // Emit different events based on payload type
                        match &event.payload {
                            SessionEventPayload::Created { session_id, .. }
                            | SessionEventPayload::Updated { session_id, .. } => {
                                let _ = self.app_handle.emit("session-updated", session_id);
                            }

                            SessionEventPayload::Completed { session_id, .. } => {
                                let _ = self.app_handle.emit("session-completed", session_id);
                            }

                            _ => {}
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(_) => continue,
                }
            }
        })
    }
}
```

**Files to create:**
- `src/events/handlers.rs`

---

### 3.5 Integrate Event Bus into Application

**File modifications:**

**`src/main.rs` - Initialize event bus:**
```rust
use events::EventBus;

fn main() {
    // ... existing setup ...

    // Create event bus (1000 event buffer)
    let event_bus = EventBus::new(1000);

    // Start event handlers
    let db_handler = DatabaseEventHandler::new(event_bus.clone());
    let db_task = db_handler.start();

    let frontend_handler = FrontendEventHandler::new(event_bus.clone(), app.handle().clone());
    let frontend_task = frontend_handler.start();

    // Store event bus in app state
    let app_state = AppState::new(event_bus.clone());

    // ... rest of setup ...
}
```

**`src/commands.rs` - Add event bus to AppState:**
```rust
pub struct AppState {
    pub watchers: Arc<Mutex<HashMap<String, Watcher>>>,
    pub upload_queue: Arc<UploadQueue>,
    pub event_bus: EventBus, // NEW
}
```

**All watcher files - Use event bus instead of direct DB calls:**
```rust
// OLD:
if let Err(e) = insert_session_immediately(...) {
    log_error(...);
}

// NEW:
if let Err(e) = event_bus.publish(PROVIDER_ID, SessionEventPayload::Created {
    session_id: file_event.session_id.clone(),
    project_name: file_event.project_name.clone(),
    file_path: file_event.path.clone(),
    file_size: file_event.file_size,
    cwd: None,
}) {
    log_error(...);
}
```

**Files to modify:**
- `src/main.rs` - Initialize event bus and handlers
- `src/commands.rs` - Add event_bus to AppState
- All 5 watcher files - Replace direct DB calls with event publishing
- `src/providers/db_helpers.rs` - Can be deprecated/removed

**Success criteria:**
- ✅ All events flow through event bus
- ✅ Database writes are transactional
- ✅ Frontend receives all events
- ✅ Events have correct sequence ordering
- ✅ All tests pass

---

### 3.6 Add Event Persistence (Optional Enhancement)

**File:** `src/events/store.rs`

```rust
/// Optional: Persist events to SQLite for debugging and replay
pub struct EventStore {
    conn: Connection,
}

impl EventStore {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                sequence INTEGER PRIMARY KEY,
                timestamp TEXT NOT NULL,
                provider TEXT NOT NULL,
                payload_type TEXT NOT NULL,
                payload JSON NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn save_event(&mut self, event: &SessionEvent) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (sequence, timestamp, provider, payload_type, payload)
             VALUES (?, ?, ?, ?, ?)",
            params![
                event.sequence,
                event.timestamp.to_rfc3339(),
                event.provider,
                event.payload_type(),
                serde_json::to_string(&event.payload)?,
            ],
        )?;
        Ok(())
    }

    pub fn get_events_since(&self, sequence: EventSequence) -> Result<Vec<SessionEvent>> {
        // Load events for replay
        todo!()
    }
}
```

**This is optional and can be done after Phase 3 core work is complete.**

---

## Phase 4: Type Safety & Polish (Priority: LOW, 2-3 days)

### 4.1 Add Newtype Wrappers

**File:** `src/types.rs` (new)

```rust
use serde::{Deserialize, Serialize};
use std::fmt;

/// Session ID newtype for type safety
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Project ID newtype
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Similar implementations for ProjectId...
```

**Files to create:**
- `src/types.rs`

**Files to modify:**
- Gradually migrate functions to use `SessionId` instead of `String`/`&str`
- Start with public APIs, work inward

**Success criteria:**
- ✅ Can't accidentally pass project_id where session_id expected
- ✅ Better autocomplete in IDE
- ✅ Clearer function signatures

---

### 4.2 Consistent Timestamp Handling

**Problem:** Mix of `i64` (millis), `DateTime<Utc>`, `Option<DateTime<Utc>>` makes it confusing.

**Solution:** Use `DateTime<Utc>` consistently, add helpers:

```rust
// src/types.rs
pub type Timestamp = DateTime<Utc>;

pub fn timestamp_to_millis(ts: &Timestamp) -> i64 {
    ts.timestamp_millis()
}

pub fn timestamp_from_millis(millis: i64) -> Timestamp {
    DateTime::from_timestamp_millis(millis)
        .expect("Invalid timestamp")
}

// For database interop
pub fn db_timestamp_to_datetime(millis: Option<i64>) -> Option<Timestamp> {
    millis.and_then(|m| DateTime::from_timestamp_millis(m))
}
```

**Files to modify:**
- `src/database.rs` - Use conversion helpers
- `src/providers/*.rs` - Use `Timestamp` type

---

### 4.3 Database Connection Improvements

**Problem:** Global static mutex, no connection pooling, no retry logic.

**Option A (Simple):** Add connection retry logic:
```rust
fn get_db_connection_with_retry() -> Result<MutexGuard<'static, Option<Connection>>> {
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(100);

    for attempt in 0..MAX_RETRIES {
        match DB_CONNECTION.lock() {
            Ok(guard) => return Ok(guard),
            Err(_) if attempt < MAX_RETRIES - 1 => {
                std::thread::sleep(RETRY_DELAY);
                continue;
            }
            Err(e) => return Err(rusqlite::Error::InvalidQuery),
        }
    }

    unreachable!()
}
```

**Option B (Better):** Use connection pool:
```rust
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

lazy_static! {
    static ref DB_POOL: Pool<SqliteConnectionManager> = {
        let manager = SqliteConnectionManager::file(get_db_path().unwrap());
        Pool::builder()
            .max_size(5)
            .build(manager)
            .expect("Failed to create connection pool")
    };
}
```

**Recommendation:** Start with Option A (retry logic), add Option B (pooling) if we see connection contention.

**Files to modify:**
- `src/database.rs` - Add retry logic or connection pool

---

### 4.4 Graceful Shutdown

**Problem:** Watchers stop abruptly, event handlers might lose events.

**Solution:** Add shutdown coordination:

```rust
// src/shutdown.rs (new)
use tokio::sync::broadcast;

pub struct ShutdownCoordinator {
    shutdown_tx: broadcast::Sender<()>,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(10);
        Self { shutdown_tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

// In watchers:
impl ClaudeWatcher {
    fn file_event_processor(..., mut shutdown_rx: broadcast::Receiver<()>) {
        loop {
            tokio::select! {
                result = rx.recv_timeout(timeout) => {
                    // Process event
                }
                _ = shutdown_rx.recv() => {
                    log_info("claude", "Graceful shutdown initiated");
                    break;
                }
            }
        }
    }
}
```

**Files to create:**
- `src/shutdown.rs`

**Files to modify:**
- `src/main.rs` - Create shutdown coordinator
- All watcher files - Listen for shutdown signal
- Event handlers - Flush events on shutdown

---

## Testing Strategy

### Phase 1 Tests
- [ ] Concurrent insert test - multiple threads insert same session
- [ ] Concurrent update test - multiple threads update same session
- [ ] Transaction rollback test - verify no partial writes
- [ ] Version conflict detection test (if using optimistic locking)

### Phase 2 Tests
- [ ] SessionState update logic test - verify cooldown and size change logic
- [ ] File filtering tests - hidden files, extensions
- [ ] Watcher integration tests - use shared components

### Phase 3 Tests
- [ ] Event bus publish/subscribe test
- [ ] Event ordering test - verify sequence numbers
- [ ] Event handler integration test - mock database and verify writes
- [ ] Event lag handling test - verify lagged events are detected
- [ ] Multiple subscriber test - ensure all handlers receive events

### Phase 4 Tests
- [ ] Type safety compilation tests - can't mix SessionId and ProjectId
- [ ] Timestamp conversion tests - round-trip millis ↔ DateTime
- [ ] Graceful shutdown test - no lost events

---

## Migration Path

### Phase 1: Can be done independently
- Low risk, high value
- No API changes
- Focus on data integrity

### Phase 2: Depends on Phase 1 completion
- Medium risk (code reorganization)
- Internal refactoring, no API changes
- Can be done provider-by-provider

### Phase 3: Depends on Phase 2 completion
- Higher risk (architectural change)
- Requires careful testing
- Can be rolled out with feature flag initially

### Phase 4: Can overlap with Phase 3
- Low risk (mostly additive)
- Gradual migration possible
- Nice-to-have improvements

---

## Success Metrics

### Code Quality
- [ ] -500 lines of duplicated code
- [ ] 100% test coverage on new modules
- [ ] No clippy warnings
- [ ] No `unwrap()` in production code paths

### Performance
- [ ] No regression in watcher latency
- [ ] Event processing < 10ms per event
- [ ] Database write latency < 50ms
- [ ] Memory usage stable (no leaks)

### Reliability
- [ ] Zero lost session updates
- [ ] Zero duplicate sessions in database
- [ ] Graceful degradation on errors
- [ ] Proper error reporting to frontend

---

## Rollback Plan

### Phase 1 Rollback
- Revert transaction changes if tests fail
- Keep backup of database file

### Phase 2 Rollback
- Keep old code in `providers/legacy/` until stable
- Feature flag to toggle old/new code path

### Phase 3 Rollback
- Keep direct database calls as fallback
- Event bus can be disabled via feature flag

### Phase 4 Rollback
- Newtype wrappers are transparent, easy to remove
- Connection pool can revert to single connection

---

## Next Steps

1. **Review this plan** - Discuss priority, scope, timeline
2. **Set up feature branch** - `rust-improvements`
3. **Start Phase 1** - Fix critical race conditions
4. **Iterate** - Review after each phase, adjust as needed

---

## Notes

- This plan assumes we have good test coverage (we should verify)
- Consider adding integration tests before starting
- May want to add metrics/observability in Phase 3
- Event bus could be extended to other parts of application (upload queue, etc.)
