/// Scanner for discovering and processing existing Cursor sessions
///
/// This runs on watcher initialization to find and process all existing
/// Cursor sessions that may not have been previously imported.
use super::{converter::CursorMessageWithRaw, db, discover_sessions, CursorSession};
use crate::events::{EventBus, SessionEventPayload};
use crate::providers::canonical::CanonicalMessage;
use crate::providers::common::get_canonical_path;
use std::fs;
use std::path::PathBuf;

const PROVIDER_ID: &str = "cursor";

/// Scan result for tracking processed sessions
#[derive(Debug)]
pub struct ScanResult {
    #[allow(dead_code)] // May be used in future for reporting
    pub sessions_found: usize,
    pub sessions_processed: usize,
    pub sessions_failed: usize,
    pub messages_converted: usize,
}

/// Statistics for message conversion
#[derive(Default)]
struct MessageStats {
    user_count: usize,
    user_json: usize,
    user_protobuf: usize,
    assistant_count: usize,
    assistant_json: usize,
    assistant_protobuf: usize,
    system_count: usize,
    other_count: usize,
    tool_use_count: usize,
    tool_result_count: usize,
    skipped_count: usize,
    failed_count: usize,
}

/// Scan all existing Cursor sessions and convert them to canonical format
///
/// This function:
/// 1. Discovers all Cursor sessions in {base_path}/chats
/// 2. Opens each session's SQLite database
/// 3. Decodes protobuf blobs
/// 4. Converts to canonical JSONL
/// 5. Writes canonical files
/// 6. Publishes SessionChanged events
///
/// Returns statistics about the scan
pub fn scan_existing_sessions(
    base_path: &std::path::Path,
    event_bus: &EventBus,
) -> Result<ScanResult, Box<dyn std::error::Error>> {
    tracing::info!("🔍 Starting Cursor session scan");

    let sessions = discover_sessions(base_path).map_err(|e| {
        let chats_dir = base_path.join("chats");
        format!(
            "Cursor is not installed or the chats directory was not found at '{}'. Please check your Cursor installation or update the home directory in settings. Original error: {}",
            chats_dir.display(),
            e
        )
    })?;

    tracing::info!("📊 Found {} Cursor sessions to scan", sessions.len());

    let mut result = ScanResult {
        sessions_found: sessions.len(),
        sessions_processed: 0,
        sessions_failed: 0,
        messages_converted: 0,
    };

    for session in sessions {
        match process_session(&session, event_bus) {
            Ok(message_count) => {
                result.sessions_processed += 1;
                result.messages_converted += message_count;

                tracing::debug!(
                    "✅ Processed session {} ({}) - {} messages",
                    session.session_id,
                    session.project_name(),
                    message_count
                );
            }
            Err(e) => {
                result.sessions_failed += 1;
                tracing::warn!(
                    "❌ Failed to process session {} ({}): {:?}",
                    session.session_id,
                    session.project_name(),
                    e
                );
            }
        }
    }

    tracing::info!(
        "✨ Cursor scan complete: {} processed, {} failed, {} messages",
        result.sessions_processed,
        result.sessions_failed,
        result.messages_converted
    );

    Ok(result)
}

/// Process a single Cursor session
///
/// Steps:
/// 1. Open database and get blobs
/// 2. Decode protobuf messages
/// 3. Convert to canonical format
/// 4. Write canonical JSONL file
/// 5. Publish SessionChanged event
///
/// Returns the number of messages converted
fn process_session(
    session: &CursorSession,
    event_bus: &EventBus,
) -> Result<usize, Box<dyn std::error::Error>> {
    // Open database
    let conn = db::open_cursor_db(&session.db_path)?;

    // Get decoded messages (supports both protobuf and JSON)
    let decoded_messages = db::get_decoded_messages(&conn)?;

    if decoded_messages.is_empty() {
        return Ok(0); // Empty session, skip
    }

    // Convert to canonical messages
    let mut canonical_messages: Vec<CanonicalMessage> = Vec::new();
    let mut stats = MessageStats::default();

    for (message_index, (_msg_id, raw_data, msg)) in decoded_messages.iter().enumerate() {
        // Track message source type
        let msg_source = match &msg {
            super::protobuf::CursorMessage::Protobuf(_) => "protobuf",
            super::protobuf::CursorMessage::Json(_) => "json",
        };

        // Wrap message with raw data and session metadata for timestamp calculation
        let msg_with_raw = CursorMessageWithRaw::new(
            msg,
            raw_data,
            session.metadata.created_at,
            message_index,
        );

        // Use split conversion to prevent UUID collisions
        match msg_with_raw.to_canonical_split() {
            Ok(messages) => {
                // Process each split message
                for mut canonical in messages {
                    // Track by role and source
                    match canonical.message.role.as_str() {
                        "user" => {
                            stats.user_count += 1;
                            if msg_source == "json" {
                                stats.user_json += 1;
                            } else {
                            stats.user_protobuf += 1;
                        }
                    }
                    "assistant" => {
                        stats.assistant_count += 1;
                        if msg_source == "json" {
                            stats.assistant_json += 1;
                        } else {
                            stats.assistant_protobuf += 1;
                        }
                    }
                    "system" => stats.system_count += 1,
                    _ => stats.other_count += 1,
                }

                // Check for tool content
                if let crate::providers::canonical::ContentValue::Structured(blocks) = &canonical.message.content {
                    for block in blocks {
                        match block {
                            crate::providers::canonical::ContentBlock::ToolUse { .. } => stats.tool_use_count += 1,
                            crate::providers::canonical::ContentBlock::ToolResult { .. } => stats.tool_result_count += 1,
                            _ => {}
                        }
                    }
                }

                // Set session ID (ToCanonical doesn't know it)
                canonical.session_id = session.session_id.clone();

                // Set CWD from session if available
                if canonical.cwd.is_none() {
                    canonical.cwd = session.cwd.clone();
                }

                    canonical_messages.push(canonical);
                }
            }
            Err(e) => {
                stats.failed_count += 1;
                tracing::warn!("Failed to convert blob in session {}: {:?}", session.session_id, e);
            }
        }
    }

    tracing::info!(
        "Conversion stats for session {}: {} messages ({} user [{} JSON + {} protobuf], {} assistant [{} JSON + {} protobuf], {} system, {} tool_use, {} tool_result, {} skipped, {} failed)",
        session.session_id,
        canonical_messages.len(),
        stats.user_count,
        stats.user_json,
        stats.user_protobuf,
        stats.assistant_count,
        stats.assistant_json,
        stats.assistant_protobuf,
        stats.system_count,
        stats.tool_use_count,
        stats.tool_result_count,
        stats.skipped_count,
        stats.failed_count
    );

    if canonical_messages.is_empty() {
        return Ok(0); // No valid messages, skip
    }

    // Sort by timestamp (though Cursor blobs don't have timestamps, use database order)
    // Messages are already in database order from the query

    // Get canonical path (use CWD if available)
    let canonical_path = get_canonical_path(
        PROVIDER_ID,
        session.cwd.as_deref(),
        &session.session_id,
    )
    .map_err(|e| -> Box<dyn std::error::Error> { Box::new(std::io::Error::other(e.to_string())) })?;

    // Write canonical JSONL
    write_canonical_file(&canonical_path, &canonical_messages)?;

    // Get file size
    let file_size = fs::metadata(&canonical_path)?.len();

    // Publish event
    let payload = SessionEventPayload::SessionChanged {
        session_id: session.session_id.clone(),
        project_name: session.project_name().to_string(),
        file_path: canonical_path.clone(),
        file_size,
    };

    event_bus.publish(PROVIDER_ID, payload)?;

    Ok(canonical_messages.len())
}

/// Write canonical messages to a JSONL file
pub fn write_canonical_file(
    path: &PathBuf,
    messages: &[CanonicalMessage],
) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Convert to JSONL (one JSON object per line)
    let jsonl: Vec<String> = messages
        .iter()
        .filter_map(|msg| serde_json::to_string(msg).ok())
        .collect();

    let content = jsonl.join("\n");

    fs::write(path, content)?;

    Ok(())
}

/// Scan Cursor sessions with optional project filtering
///
/// This function matches the API of other providers for use in session_scanner.rs dispatcher.
/// Uses base_path to locate Cursor's chats and projects directories.
pub fn scan_sessions_filtered(
    base_path: &std::path::Path,
    selected_projects: Option<&[String]>,
) -> Result<Vec<crate::providers::common::SessionInfo>, String> {
    use crate::logging::{log_info, log_warn};

    // Discover all Cursor sessions
    let sessions = discover_sessions(base_path).map_err(|e| {
        let chats_dir = base_path.join("chats");
        format!(
            "Cursor is not installed or the chats directory was not found at '{}'. Please check your Cursor installation or update the home directory in settings. Error: {}",
            chats_dir.display(),
            e
        )
    })?;

    if let Err(e) = log_info("cursor", &format!("📊 Found {} Cursor sessions to scan", sessions.len())) {
        eprintln!("Logging error: {}", e);
    }

    let mut session_infos = Vec::new();

    for session in sessions {
        match scan_single_cursor_session(&session, selected_projects) {
            Ok(Some(info)) => session_infos.push(info),
            Ok(None) => {
                // Session filtered out - skipped
            }
            Err(e) => {
                if let Err(log_err) = log_warn(
                    "cursor",
                    &format!("Failed to scan Cursor session {}: {}", session.session_id, e),
                ) {
                    eprintln!("Logging error: {}", log_err);
                }
            }
        }
    }

    if let Err(e) = log_info("cursor", &format!("✅ Scanned {} Cursor sessions", session_infos.len())) {
        eprintln!("Logging error: {}", e);
    }

    Ok(session_infos)
}

fn scan_single_cursor_session(
    session: &CursorSession,
    selected_projects: Option<&[String]>,
) -> Result<Option<crate::providers::common::SessionInfo>, String> {
    use crate::providers::common::SessionInfo;
    use chrono::{DateTime, Utc};
    use std::path::Path;

    // Open database and get decoded messages (supports both protobuf and JSON)
    let conn = db::open_cursor_db(&session.db_path).map_err(|e| e.to_string())?;
    let decoded_messages = db::get_decoded_messages(&conn).map_err(|e| e.to_string())?;

    if decoded_messages.is_empty() {
        return Err("Empty session (no messages)".to_string());
    }

    // Convert messages to canonical format (decoded_messages is Vec<(String, Vec<u8>, CursorMessage)>)
    let mut canonical_messages = Vec::new();

    for (message_index, (_blob_id, raw_data, msg)) in decoded_messages.iter().enumerate() {
        // Wrap message with raw data and session metadata for timestamp calculation
        let msg_with_raw = CursorMessageWithRaw::new(
            msg,
            raw_data,
            session.metadata.created_at,
            message_index,
        );

        // Use to_canonical_split() to properly separate tool calls and tool results
        if let Ok(messages) = msg_with_raw.to_canonical_split() {
            for mut canonical_msg in messages {
                // Set session ID (required for UI parser)
                canonical_msg.session_id = session.session_id.clone();

                // Use CWD from session (already discovered)
                if canonical_msg.cwd.is_none() {
                    canonical_msg.cwd = session.cwd.clone();
                }
                canonical_messages.push(canonical_msg);
            }
        }
    }

    if canonical_messages.is_empty() {
        return Err("No valid messages after conversion".to_string());
    }

    // Sort by timestamp
    canonical_messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // Extract CWD from first message (now enriched with project directory lookup)
    let cwd = canonical_messages
        .first()
        .and_then(|m| m.cwd.clone());

    // Derive project name from CWD (last path component) or fall back to session name
    let project_name = cwd
        .as_ref()
        .and_then(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| session.metadata.name.clone());

    // Filter projects BEFORE processing/caching
    if let Some(selected) = selected_projects {
        if !selected.contains(&project_name) {
            return Ok(None); // Skip this session
        }
    }

    // Get canonical path
    let canonical_path = get_canonical_path("cursor", cwd.as_deref(), &session.session_id)
        .map_err(|e| format!("Failed to get canonical path: {}", e))?;

    // Write canonical JSONL
    let canonical_content = canonical_messages
        .iter()
        .filter_map(|msg| serde_json::to_string(msg).ok())
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(&canonical_path, canonical_content)
        .map_err(|e| format!("Failed to write canonical file: {}", e))?;

    // Extract timing from messages
    let session_start_time = canonical_messages
        .first()
        .and_then(|m| DateTime::parse_from_rfc3339(&m.timestamp).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let session_end_time = canonical_messages
        .last()
        .and_then(|m| DateTime::parse_from_rfc3339(&m.timestamp).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let duration_ms = if let (Some(start), Some(end)) = (session_start_time, session_end_time) {
        Some((end - start).num_milliseconds())
    } else {
        None
    };

    // Get file size
    let file_size = fs::metadata(&canonical_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let file_name = canonical_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.jsonl")
        .to_string();

    Ok(Some(SessionInfo {
        provider: "cursor".to_string(),
        project_name, // Use derived project name from CWD
        session_id: session.session_id.clone(),
        file_path: canonical_path,
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None,
        cwd,
        project_hash: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_canonical_file() {
        use crate::providers::canonical::{ContentValue, MessageContent, MessageType};
        use chrono::Utc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.jsonl");

        let messages = vec![CanonicalMessage {
            uuid: "test-1".to_string(),
            timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            message_type: MessageType::User,
            session_id: "test-session".to_string(),
            provider: "cursor".to_string(),
            cwd: Some("/test".to_string()),
            git_branch: None,
            version: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            message: MessageContent {
                role: "user".to_string(),
                content: ContentValue::Text("Test message".to_string()),
                model: None,
                usage: None,
            },
            provider_metadata: None,
            is_meta: None,
            request_id: None,
            tool_use_result: None,
        }];

        write_canonical_file(&file_path, &messages).unwrap();

        assert!(file_path.exists());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Test message"));
        assert!(content.contains("cursor"));
    }
}
