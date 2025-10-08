use crate::logging::{log_debug, log_info};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Emitter;
use uuid::Uuid;

lazy_static! {
    static ref DB_CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);
    static ref APP_HANDLE: Mutex<Option<tauri::AppHandle>> = Mutex::new(None);
}

/// Helper function to get database connection with proper error handling
/// Replaces repeated `.lock().unwrap()` pattern throughout the code
fn get_db_connection(
) -> Result<std::sync::MutexGuard<'static, Option<Connection>>, rusqlite::Error> {
    DB_CONNECTION
        .lock()
        .map_err(|_| rusqlite::Error::InvalidQuery)
}

/// Helper function to get the active database connection with mutable access
/// Returns an error if the connection is not initialized
fn with_connection_mut<F, T>(f: F) -> Result<T, rusqlite::Error>
where
    F: FnOnce(&mut Connection) -> Result<T, rusqlite::Error>,
{
    let mut db_conn = get_db_connection()?;
    let conn = db_conn
        .as_mut()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;
    f(conn)
}

/// Set the app handle for event emission
pub fn set_app_handle(app_handle: tauri::AppHandle) {
    if let Ok(mut handle_guard) = APP_HANDLE.lock() {
        *handle_guard = Some(app_handle);
    }
}

/// Initialize the database connection
/// Note: Migrations are handled by tauri-plugin-sql
pub fn init_database() -> Result<()> {
    let db_path = get_db_path()?;

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_e| rusqlite::Error::InvalidPath(parent.to_path_buf()))?;
    }

    // Open connection to existing database (migrations handled by plugin)
    let conn = Connection::open(&db_path)?;

    // Store connection
    let mut db_conn = DB_CONNECTION.lock().unwrap();
    *db_conn = Some(conn);

    log_info(
        "database",
        &format!("✓ Database connection established at {:?}", db_path),
    )
    .unwrap_or_default();

    Ok(())
}

/// Get the database file path (same location as tauri-plugin-sql uses)
fn get_db_path() -> Result<std::path::PathBuf> {
    // Use Tauri's app data directory (same as plugin)
    // On macOS: ~/Library/Application Support/com.guideai.desktop/
    // On Linux: ~/.local/share/com.guideai.desktop/
    // On Windows: %APPDATA%/com.guideai.desktop/
    let app_dir = dirs::data_local_dir()
        .ok_or_else(|| rusqlite::Error::InvalidPath(std::path::PathBuf::from("app_data")))?
        .join("com.guideai.desktop");

    Ok(app_dir.join("guideai.db"))
}

/// Insert a session into the database
pub fn insert_session(
    provider: &str,
    project_name: &str,
    session_id: &str,
    file_name: &str,
    file_path: &str,
    file_size: u64,
    session_start_time: Option<DateTime<Utc>>,
    session_end_time: Option<DateTime<Utc>>,
    duration_ms: Option<i64>,
    cwd: Option<&str>,
) -> Result<String> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    // Check if session is complete (has end time)
    let session_completed = session_end_time.is_some();

    conn.execute(
        "INSERT INTO agent_sessions (
            id, provider, project_name, session_id, file_name, file_path, file_size,
            session_start_time, session_end_time, duration_ms, cwd,
            processing_status, synced_to_server,
            created_at, uploaded_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', 0, ?, ?)",
        params![
            id,
            provider,
            project_name,
            session_id,
            file_name,
            file_path,
            file_size as i64,
            session_start_time.map(|t| t.timestamp_millis()),
            session_end_time.map(|t| t.timestamp_millis()),
            duration_ms,
            cwd,
            now,
            now,
        ],
    )?;

    log_info(
        "database",
        &format!("✓ Inserted session {} into local database", session_id),
    )
    .unwrap_or_default();

    // Emit event to frontend
    if let Ok(app_handle_guard) = APP_HANDLE.lock() {
        if let Some(ref app_handle) = *app_handle_guard {
            let _ = app_handle.emit("session-updated", session_id);

            // Emit session-completed event if session already has end time
            if session_completed {
                let _ = app_handle.emit("session-completed", session_id);
                log_info(
                    "database",
                    &format!("✓ Session {} completed on insert, emitted event for metrics processing", session_id),
                )
                .unwrap_or_default();
            }
        }
    }

    Ok(id)
}

/// Update an existing session with new activity (file size, timestamp)
pub fn update_session(
    session_id: &str,
    _file_name: &str,  // Kept for API compatibility but not used in query
    file_size: u64,
    session_start_time: Option<DateTime<Utc>>,
    session_end_time: Option<DateTime<Utc>>,
    cwd: Option<&str>,
) -> Result<()> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let now = Utc::now().timestamp_millis();

    // Get the existing start time, end time, and cwd from database
    // Query by session_id only since providers like OpenCode have multiple files per session
    let (existing_start_time_ms, existing_end_time_ms, existing_cwd): (Option<i64>, Option<i64>, Option<String>) = conn.query_row(
        "SELECT session_start_time, session_end_time, cwd FROM agent_sessions WHERE session_id = ?",
        params![session_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).ok().unwrap_or((None, None, None));

    // Use new start time if provided and existing is null, otherwise keep existing
    let final_start_time_ms = match (existing_start_time_ms, session_start_time) {
        (None, Some(new_start)) => Some(new_start.timestamp_millis()), // Database has null, use new value
        (Some(existing), _) => Some(existing), // Keep existing non-null value
        (None, None) => None,                  // Both null, stay null
    };

    // Use new cwd if provided and existing is null, otherwise keep existing
    let final_cwd = match (existing_cwd, cwd) {
        (None, Some(new_cwd)) => Some(new_cwd.to_string()), // Database has null, use new value
        (Some(existing), _) => Some(existing),              // Keep existing non-null value
        (None, None) => None,                               // Both null, stay null
    };

    // Calculate duration if we have both start and end times
    let duration_ms = if let (Some(start), Some(end)) = (final_start_time_ms, session_end_time) {
        Some((end.timestamp_millis() - start).max(0))
    } else {
        None
    };

    // Detect if session is being completed (first time getting end time)
    let session_completed = existing_end_time_ms.is_none() && session_end_time.is_some();

    // Update by session_id only since providers like OpenCode have multiple files per session
    // Reset core_metrics_status and processing_status to 'pending' since file content has changed
    conn.execute(
        "UPDATE agent_sessions
         SET file_size = ?,
             session_start_time = ?,
             session_end_time = ?,
             duration_ms = ?,
             cwd = ?,
             uploaded_at = ?,
             synced_to_server = 0,
             core_metrics_status = 'pending',
             processing_status = 'pending'
         WHERE session_id = ?",
        params![
            file_size as i64,
            final_start_time_ms,
            session_end_time.map(|t| t.timestamp_millis()),
            duration_ms,
            final_cwd,
            now,
            session_id,
        ],
    )?;

    log_debug(
        "database",
        &format!(
            "↻ Updated session {} (size: {} bytes, needs re-sync)",
            session_id, file_size
        ),
    )
    .unwrap_or_default();

    // Emit event to frontend
    if let Ok(app_handle_guard) = APP_HANDLE.lock() {
        if let Some(ref app_handle) = *app_handle_guard {
            let _ = app_handle.emit("session-updated", session_id);

            // Emit session-completed event if this is the first time the session got an end time
            if session_completed {
                let _ = app_handle.emit("session-completed", session_id);
                log_info(
                    "database",
                    &format!("✓ Session {} completed, emitted event for metrics processing", session_id),
                )
                .unwrap_or_default();
            }
        }
    }

    Ok(())
}

/// Check if a session already exists in the database
pub fn session_exists(session_id: &str, _file_name: &str) -> Result<bool> {  // Kept for API compatibility but not used in query
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    // Check by session_id only since providers like OpenCode have multiple files per session
    // The session_id field has a UNIQUE constraint, so we only need to check that
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agent_sessions WHERE session_id = ?",
        params![session_id],
        |row| row.get(0),
    )?;

    Ok(count > 0)
}

/// Get all unsynced sessions (for upload queue)
/// Only returns sessions that have both start and end times, no sync failure,
/// and where the provider's sync mode is set to "Transcript and Metrics" or "Metrics Only"
/// For "Metrics Only" mode, requires core_metrics_status = 'completed' (uploads twice: first with core metrics, then with AI)
pub fn get_unsynced_sessions() -> Result<Vec<UnsyncedSession>> {
    use crate::config::load_provider_config;

    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let mut stmt = conn.prepare(
        "SELECT id, provider, project_name, session_id, file_name, file_path, file_size, cwd,
                session_start_time, session_end_time,
                COALESCE(core_metrics_status, 'pending') as core_metrics_status,
                COALESCE(processing_status, 'pending') as processing_status
         FROM agent_sessions
         WHERE synced_to_server = 0
           AND session_start_time IS NOT NULL
           AND session_end_time IS NOT NULL
           AND sync_failed_reason IS NULL
         ORDER BY created_at ASC",
    )?;

    let all_sessions = stmt
        .query_map([], |row| {
            Ok((
                UnsyncedSession {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    project_name: row.get(2)?,
                    session_id: row.get(3)?,
                    file_name: row.get(4)?,
                    file_path: row.get(5)?,
                    file_size: row.get(6)?,
                    cwd: row.get(7)?,
                    session_start_time: row.get(8)?,
                    session_end_time: row.get(9)?,
                },
                row.get::<_, String>(10)?, // core_metrics_status
                row.get::<_, String>(11)?, // processing_status
            ))
        })?
        .collect::<Result<Vec<_>>>()?;

    // Filter to include sessions with sync mode "Transcript and Metrics" or "Metrics Only"
    // For "Metrics Only", require core_metrics_status = 'completed' (will upload twice: first with core, then with AI)
    let sessions = all_sessions
        .into_iter()
        .filter_map(|(session, core_metrics_status, _processing_status)| {
            match load_provider_config(&session.provider) {
                Ok(config) => {
                    if config.sync_mode == "Transcript and Metrics" {
                        // Transcript mode: upload anytime after session ends
                        Some(session)
                    } else if config.sync_mode == "Metrics Only" {
                        // Metrics Only: wait for core metrics to complete (uploads immediately after core metrics)
                        // Will upload again later when AI processing completes (server upserts)
                        if core_metrics_status == "completed" {
                            Some(session)
                        } else {
                            None
                        }
                    } else {
                        // Other sync modes (e.g., "Nothing"): don't sync
                        None
                    }
                }
                Err(_) => {
                    // If we can't load config, default to not syncing (safe default)
                    None
                }
            }
        })
        .collect();

    Ok(sessions)
}

/// Mark a session as synced
pub fn mark_session_synced(session_id: &str, server_session_id: Option<&str>) -> Result<()> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let now = Utc::now().timestamp_millis();

    conn.execute(
        "UPDATE agent_sessions
         SET synced_to_server = 1, synced_at = ?, server_session_id = ?, sync_failed_reason = NULL
         WHERE session_id = ?",
        params![now, server_session_id, session_id],
    )?;

    log_info(
        "database",
        &format!("✓ Marked session {} as synced", session_id),
    )
    .unwrap_or_default();

    Ok(())
}

/// Mark a session as sync failed with reason
pub fn mark_session_sync_failed(session_id: &str, reason: &str) -> Result<()> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    conn.execute(
        "UPDATE agent_sessions
         SET sync_failed_reason = ?
         WHERE session_id = ?",
        params![reason, session_id],
    )?;

    log_info(
        "database",
        &format!("✗ Marked session {} as sync failed: {}", session_id, reason),
    )
    .unwrap_or_default();

    Ok(())
}

#[derive(Debug)]
pub struct FailedSession {
    pub id: String,
    pub provider: String,
    pub project_name: String,
    pub session_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: i64,
    pub cwd: Option<String>,
    pub sync_failed_reason: String,
}

/// Get all failed sessions (for upload queue display)
pub fn get_failed_sessions() -> Result<Vec<FailedSession>> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let mut stmt = conn.prepare(
        "SELECT id, provider, project_name, session_id, file_name, file_path, file_size, cwd, sync_failed_reason
         FROM agent_sessions
         WHERE sync_failed_reason IS NOT NULL
         ORDER BY created_at DESC"
    )?;

    let sessions = stmt
        .query_map([], |row| {
            Ok(FailedSession {
                id: row.get(0)?,
                provider: row.get(1)?,
                project_name: row.get(2)?,
                session_id: row.get(3)?,
                file_name: row.get(4)?,
                file_path: row.get(5)?,
                file_size: row.get(6)?,
                cwd: row.get(7)?,
                sync_failed_reason: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;

    Ok(sessions)
}

/// Get upload statistics from database (real-time)
/// Pending count only includes sessions from providers with sync mode "Transcript and Metrics"
pub fn get_upload_stats() -> Result<UploadStats> {
    // Use get_unsynced_sessions which already filters by sync mode
    let unsynced = get_unsynced_sessions()?;
    let pending = unsynced.len();

    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    // Count synced sessions
    let synced: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agent_sessions WHERE synced_to_server = 1",
        [],
        |row| row.get(0),
    )?;

    // Count total sessions
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM agent_sessions", [], |row| row.get(0))?;

    Ok(UploadStats {
        pending,
        synced: synced as usize,
        total: total as usize,
    })
}

#[derive(Debug, Clone)]
pub struct UploadStats {
    pub pending: usize,
    #[allow(dead_code)]
    pub synced: usize,
    #[allow(dead_code)]
    pub total: usize,
}

#[derive(Debug)]
pub struct UnsyncedSession {
    pub id: String,
    pub provider: String,
    pub project_name: String,
    pub session_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: i64,
    pub cwd: Option<String>,
    #[allow(dead_code)]
    pub session_start_time: Option<i64>,
    #[allow(dead_code)]
    pub session_end_time: Option<i64>,
}

/// Insert or get a project by CWD (upsert)
/// Uses a transaction to ensure atomicity
pub fn insert_or_get_project(
    name: &str,
    github_repo: Option<&str>,
    cwd: &str,
    project_type: &str,
) -> Result<String> {
    with_connection_mut(|conn| {
        // Use a transaction for atomic upsert
        let tx = conn.transaction()?;

        let now = Utc::now().timestamp_millis();

        // Try to get existing project by CWD
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM projects WHERE cwd = ?",
                params![cwd],
                |row| row.get(0),
            )
            .ok();

        let project_id = if let Some(project_id) = existing {
            // Update existing project
            tx.execute(
                "UPDATE projects SET name = ?, github_repo = ?, type = ?, updated_at = ? WHERE id = ?",
                params![name, github_repo, project_type, now, project_id],
            )?;

            log_debug(
                "database",
                &format!("↻ Updated project {} ({})", name, project_id),
            )
            .unwrap_or_default();

            project_id
        } else {
            // Insert new project
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO projects (id, name, github_repo, cwd, type, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![id, name, github_repo, cwd, project_type, now, now],
            )?;

            log_info("database", &format!("✓ Inserted project {} ({})", name, &id))
                .unwrap_or_default();

            // Emit event to frontend
            if let Ok(app_handle_guard) = APP_HANDLE.lock() {
                if let Some(ref app_handle) = *app_handle_guard {
                    let _ = app_handle.emit("project-updated", &id);
                }
            }

            id
        };

        // Commit transaction
        tx.commit()?;

        Ok(project_id)
    })
}

/// Get all projects with session counts
pub fn get_all_projects() -> Result<Vec<ProjectWithCount>> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.github_repo, p.cwd, p.type, p.created_at, p.updated_at,
                COUNT(s.id) as session_count
         FROM projects p
         LEFT JOIN agent_sessions s ON p.id = s.project_id
         GROUP BY p.id
         ORDER BY p.updated_at DESC",
    )?;

    let projects = stmt
        .query_map([], |row| {
            Ok(ProjectWithCount {
                id: row.get(0)?,
                name: row.get(1)?,
                github_repo: row.get(2)?,
                cwd: row.get(3)?,
                project_type: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                session_count: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;

    Ok(projects)
}

/// Get a single project by ID
pub fn get_project_by_id(project_id: &str) -> Result<Option<ProjectWithCount>> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let project: Option<ProjectWithCount> = conn
        .query_row(
            "SELECT p.id, p.name, p.github_repo, p.cwd, p.type, p.created_at, p.updated_at,
                COUNT(s.id) as session_count
         FROM projects p
         LEFT JOIN agent_sessions s ON p.id = s.project_id
         WHERE p.id = ?
         GROUP BY p.id",
            params![project_id],
            |row| {
                Ok(ProjectWithCount {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    github_repo: row.get(2)?,
                    cwd: row.get(3)?,
                    project_type: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    session_count: row.get(7)?,
                })
            },
        )
        .ok();

    Ok(project)
}

/// Attach a session to a project
pub fn attach_session_to_project(session_id: &str, project_id: &str) -> Result<()> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    conn.execute(
        "UPDATE agent_sessions SET project_id = ? WHERE session_id = ?",
        params![project_id, session_id],
    )?;

    log_debug(
        "database",
        &format!(
            "↻ Attached session {} to project {}",
            session_id, project_id
        ),
    )
    .unwrap_or_default();

    Ok(())
}

#[derive(Debug, Clone)]
pub struct ProjectWithCount {
    pub id: String,
    pub name: String,
    pub github_repo: Option<String>,
    pub cwd: String,
    pub project_type: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub session_count: i64,
}

/// Execute a raw SQL query and return results as JSON
/// This is used by the React frontend to query the database dynamically
pub fn execute_sql_query(
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let mut stmt = conn.prepare(sql)?;

    // Convert JSON params to rusqlite params
    let rusqlite_params: Vec<Box<dyn rusqlite::ToSql>> = params
        .iter()
        .map(|p| match p {
            serde_json::Value::String(s) => Box::new(s.clone()) as Box<dyn rusqlite::ToSql>,
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Box::new(i) as Box<dyn rusqlite::ToSql>
                } else if let Some(f) = n.as_f64() {
                    Box::new(f) as Box<dyn rusqlite::ToSql>
                } else {
                    Box::new(rusqlite::types::Null) as Box<dyn rusqlite::ToSql>
                }
            }
            serde_json::Value::Bool(b) => Box::new(*b) as Box<dyn rusqlite::ToSql>,
            serde_json::Value::Null => Box::new(rusqlite::types::Null) as Box<dyn rusqlite::ToSql>,
            _ => Box::new(rusqlite::types::Null) as Box<dyn rusqlite::ToSql>,
        })
        .collect();

    let param_refs: Vec<&dyn rusqlite::ToSql> =
        rusqlite_params.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let mut map = serde_json::Map::new();
        let column_count = row.as_ref().column_count();

        for i in 0..column_count {
            let column_name = row.as_ref().column_name(i)?.to_string();
            let value: serde_json::Value = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                rusqlite::types::ValueRef::Integer(i) => serde_json::Value::Number(i.into()),
                rusqlite::types::ValueRef::Real(f) => serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
                rusqlite::types::ValueRef::Text(s) => {
                    serde_json::Value::String(String::from_utf8_lossy(s).to_string())
                }
                rusqlite::types::ValueRef::Blob(b) => {
                    serde_json::Value::String(general_purpose::STANDARD.encode(b))
                }
            };
            map.insert(column_name, value);
        }

        Ok(serde_json::Value::Object(map))
    })?;

    rows.collect()
}

/// Quick rate a session with thumbs up/meh/thumbs down
pub fn quick_rate_session(session_id: &str, rating: &str) -> Result<()> {
    log_info(
        "database",
        &format!("Quick rating session {} with {}", session_id, rating),
    )
    .unwrap_or_default();

    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let now = Utc::now().timestamp_millis();

    // Check if assessment already exists
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM session_assessments WHERE session_id = ?",
            params![session_id],
            |row| row.get(0),
        )
        .ok();

    log_debug("database", &format!("Existing assessment: {:?}", existing)).unwrap_or_default();

    if let Some(id) = existing {
        // Update existing assessment with new rating
        conn.execute(
            "UPDATE session_assessments SET rating = ? WHERE id = ?",
            params![rating, id],
        )?;

        log_debug(
            "database",
            &format!("↻ Updated rating for session {}: {}", session_id, rating),
        )
        .unwrap_or_default();
    } else {
        // Create new minimal assessment with just the rating
        let assessment_id = Uuid::new_v4().to_string();

        // Get provider from agent_sessions
        let provider: String = conn.query_row(
            "SELECT provider FROM agent_sessions WHERE session_id = ?",
            params![session_id],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT INTO session_assessments (id, session_id, provider, responses, rating, completed_at, created_at)
             VALUES (?, ?, ?, '{}', ?, ?, ?)",
            params![assessment_id, session_id, provider, rating, now, now],
        )?;

        log_info(
            "database",
            &format!("✓ Created rating for session {}: {}", session_id, rating),
        )
        .unwrap_or_default();
    }

    // Update agent_sessions assessment_status to 'rating_only' and set completed time
    conn.execute(
        "UPDATE agent_sessions SET assessment_status = 'rating_only', assessment_completed_at = ? WHERE session_id = ?",
        params![now, session_id],
    )?;

    // Emit event to frontend
    if let Ok(app_handle_guard) = APP_HANDLE.lock() {
        if let Some(ref app_handle) = *app_handle_guard {
            let _ = app_handle.emit("session-updated", session_id);
        }
    }

    Ok(())
}

/// Get the rating for a session
pub fn get_session_rating(session_id: &str) -> Result<Option<String>> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let rating: Option<String> = conn
        .query_row(
            "SELECT rating FROM session_assessments WHERE session_id = ?",
            params![session_id],
            |row| row.get(0),
        )
        .ok();

    Ok(rating)
}

/// Full session data structure for metrics-only sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullSessionData {
    pub session_id: String,
    pub provider: String,
    pub project_name: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: i64,
    pub session_start_time: Option<i64>,
    pub session_end_time: Option<i64>,
    pub duration_ms: Option<i64>,
    pub processing_status: String,
    pub queued_at: Option<i64>,
    pub processed_at: Option<i64>,
    pub assessment_status: String,
    pub assessment_completed_at: Option<i64>,
    pub ai_model_summary: Option<String>,
    pub ai_model_quality_score: Option<i64>,
    pub ai_model_metadata: Option<String>,
    pub ai_model_phase_analysis: Option<String>,
}

/// Get full session data by session ID (for metrics-only sync)
pub fn get_full_session_by_id(session_id: &str) -> Result<Option<FullSessionData>> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let session: Option<FullSessionData> = conn
        .query_row(
            "SELECT session_id, provider, project_name, file_name, file_path, file_size,
                    session_start_time, session_end_time, duration_ms,
                    processing_status, queued_at, processed_at,
                    assessment_status, assessment_completed_at,
                    ai_model_summary, ai_model_quality_score, ai_model_metadata, ai_model_phase_analysis
             FROM agent_sessions
             WHERE session_id = ?",
            params![session_id],
            |row| {
                Ok(FullSessionData {
                    session_id: row.get(0)?,
                    provider: row.get(1)?,
                    project_name: row.get(2)?,
                    file_name: row.get(3)?,
                    file_path: row.get(4)?,
                    file_size: row.get(5)?,
                    session_start_time: row.get(6)?,
                    session_end_time: row.get(7)?,
                    duration_ms: row.get(8)?,
                    processing_status: row.get(9)?,
                    queued_at: row.get(10)?,
                    processed_at: row.get(11)?,
                    assessment_status: row.get(12)?,
                    assessment_completed_at: row.get(13)?,
                    ai_model_summary: row.get(14)?,
                    ai_model_quality_score: row.get(15)?,
                    ai_model_metadata: row.get(16)?,
                    ai_model_phase_analysis: row.get(17)?,
                })
            },
        )
        .ok();

    Ok(session)
}

/// Session metrics structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetrics {
    pub session_id: String,
    pub provider: String,
    // Performance metrics
    pub response_latency_ms: Option<f64>,
    pub task_completion_time_ms: Option<f64>,
    pub performance_total_responses: Option<i64>,
    // Usage metrics
    pub read_write_ratio: Option<f64>,
    pub input_clarity_score: Option<f64>,
    pub read_operations: Option<i64>,
    pub write_operations: Option<i64>,
    pub total_user_messages: Option<i64>,
    // Error metrics
    pub error_count: Option<i64>,
    pub error_types: Option<String>,
    pub last_error_message: Option<String>,
    pub recovery_attempts: Option<i64>,
    pub fatal_errors: Option<i64>,
    // Engagement metrics
    pub interruption_rate: Option<f64>,
    pub session_length_minutes: Option<f64>,
    pub total_interruptions: Option<i64>,
    pub engagement_total_responses: Option<i64>,
    // Quality metrics
    pub task_success_rate: Option<f64>,
    pub iteration_count: Option<i64>,
    pub process_quality_score: Option<f64>,
    pub used_plan_mode: Option<bool>,
    pub used_todo_tracking: Option<bool>,
    pub over_top_affirmations: Option<i64>,
    pub successful_operations: Option<i64>,
    pub total_operations: Option<i64>,
    pub exit_plan_mode_count: Option<i64>,
    pub todo_write_count: Option<i64>,
    pub over_top_affirmations_phrases: Option<String>,
    pub improvement_tips: Option<String>,
    pub custom_metrics: Option<String>,
}

/// Clear all failed sessions from the database
pub fn clear_failed_sessions() -> Result<()> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    conn.execute(
        "DELETE FROM agent_sessions WHERE sync_failed_reason IS NOT NULL",
        [],
    )?;

    log_info("database", "✓ Cleared all failed sessions from database").unwrap_or_default();

    Ok(())
}

/// Retry all failed sessions by resetting their sync status
pub fn retry_failed_sessions() -> Result<()> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    conn.execute(
        "UPDATE agent_sessions
         SET sync_failed_reason = NULL, synced_to_server = 0
         WHERE sync_failed_reason IS NOT NULL",
        [],
    )?;

    log_info("database", "✓ Retrying all failed sessions").unwrap_or_default();

    Ok(())
}

/// Remove a session from the database by ID
pub fn remove_session_by_id(session_id: &str) -> Result<usize> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let rows_affected = conn.execute(
        "DELETE FROM agent_sessions WHERE id = ?",
        params![session_id],
    )?;

    if rows_affected > 0 {
        log_info("database", &format!("✓ Removed session {} from database", session_id)).unwrap_or_default();
    }

    Ok(rows_affected)
}

/// Retry a single failed session by resetting its sync status
pub fn retry_session_by_id(session_id: &str) -> Result<usize> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let rows_affected = conn.execute(
        "UPDATE agent_sessions
         SET sync_failed_reason = NULL, synced_to_server = 0
         WHERE id = ? AND sync_failed_reason IS NOT NULL",
        params![session_id],
    )?;

    if rows_affected > 0 {
        log_info("database", &format!("✓ Retrying session {}", session_id)).unwrap_or_default();
    }

    Ok(rows_affected)
}

/// Get session metrics by session ID
pub fn get_session_metrics(session_id: &str) -> Result<Option<SessionMetrics>> {
    let db_conn = DB_CONNECTION.lock().unwrap();
    let conn = db_conn
        .as_ref()
        .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

    let metrics: Option<SessionMetrics> = conn
        .query_row(
            "SELECT session_id, provider,
                    response_latency_ms, task_completion_time_ms, performance_total_responses,
                    read_write_ratio, input_clarity_score, read_operations, write_operations, total_user_messages,
                    error_count, error_types, last_error_message, recovery_attempts, fatal_errors,
                    interruption_rate, session_length_minutes, total_interruptions, engagement_total_responses,
                    task_success_rate, iteration_count, process_quality_score,
                    used_plan_mode, used_todo_tracking, over_top_affirmations,
                    successful_operations, total_operations, exit_plan_mode_count, todo_write_count,
                    over_top_affirmations_phrases, improvement_tips, custom_metrics
             FROM session_metrics
             WHERE session_id = ?
             ORDER BY created_at DESC
             LIMIT 1",
            params![session_id],
            |row| {
                Ok(SessionMetrics {
                    session_id: row.get(0)?,
                    provider: row.get(1)?,
                    response_latency_ms: row.get(2)?,
                    task_completion_time_ms: row.get(3)?,
                    performance_total_responses: row.get(4)?,
                    read_write_ratio: row.get(5)?,
                    input_clarity_score: row.get(6)?,
                    read_operations: row.get(7)?,
                    write_operations: row.get(8)?,
                    total_user_messages: row.get(9)?,
                    error_count: row.get(10)?,
                    error_types: row.get(11)?,
                    last_error_message: row.get(12)?,
                    recovery_attempts: row.get(13)?,
                    fatal_errors: row.get(14)?,
                    interruption_rate: row.get(15)?,
                    session_length_minutes: row.get(16)?,
                    total_interruptions: row.get(17)?,
                    engagement_total_responses: row.get(18)?,
                    task_success_rate: row.get(19)?,
                    iteration_count: row.get(20)?,
                    process_quality_score: row.get(21)?,
                    used_plan_mode: row.get::<_, Option<i64>>(22)?.map(|v| v != 0),
                    used_todo_tracking: row.get::<_, Option<i64>>(23)?.map(|v| v != 0),
                    over_top_affirmations: row.get(24)?,
                    successful_operations: row.get(25)?,
                    total_operations: row.get(26)?,
                    exit_plan_mode_count: row.get(27)?,
                    todo_write_count: row.get(28)?,
                    over_top_affirmations_phrases: row.get(29)?,
                    improvement_tips: row.get(30)?,
                    custom_metrics: row.get(31)?,
                })
            },
        )
        .ok();

    Ok(metrics)
}
