/// Scanner for discovering and processing existing Cursor sessions
///
/// This runs on watcher initialization to find and process all existing
/// Cursor sessions that may not have been previously imported.

use super::{db, discover_sessions, protobuf::CursorBlob, CursorSession};
use crate::events::{EventBus, SessionEventPayload};
use crate::providers::canonical::{converter::ToCanonical, CanonicalMessage};
use crate::providers::common::get_canonical_path;
use std::fs;
use std::path::PathBuf;

const PROVIDER_ID: &str = "cursor";

/// Scan result for tracking processed sessions
#[derive(Debug)]
pub struct ScanResult {
    pub sessions_found: usize,
    pub sessions_processed: usize,
    pub sessions_failed: usize,
    pub messages_converted: usize,
}

/// Scan all existing Cursor sessions and convert them to canonical format
///
/// This function:
/// 1. Discovers all Cursor sessions in ~/.cursor/chats
/// 2. Opens each session's SQLite database
/// 3. Decodes protobuf blobs
/// 4. Converts to canonical JSONL
/// 5. Writes canonical files
/// 6. Publishes SessionChanged events
///
/// Returns statistics about the scan
pub fn scan_existing_sessions(
    event_bus: &EventBus,
) -> Result<ScanResult, Box<dyn std::error::Error>> {
    tracing::info!("🔍 Starting Cursor session scan");

    let sessions = discover_sessions()?;

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

    // Get decoded blobs
    let decoded_blobs = db::get_decoded_blobs(&conn)?;

    if decoded_blobs.is_empty() {
        return Ok(0); // Empty session, skip
    }

    // Convert to canonical messages
    let mut canonical_messages: Vec<CanonicalMessage> = Vec::new();

    for (_blob_id, blob) in decoded_blobs {
        match blob.to_canonical() {
            Ok(Some(mut canonical)) => {
                // Set session ID (ToCanonical doesn't know it)
                canonical.session_id = session.session_id.clone();

                // Set CWD to project name if not set
                if canonical.cwd.is_none() {
                    canonical.cwd = Some(session.project_name().to_string());
                }

                canonical_messages.push(canonical);
            }
            Ok(None) => {
                // Message was skipped (empty, etc.)
            }
            Err(e) => {
                tracing::warn!("Failed to convert blob in session {}: {:?}", session.session_id, e);
            }
        }
    }

    if canonical_messages.is_empty() {
        return Ok(0); // No valid messages, skip
    }

    // Sort by timestamp (though Cursor blobs don't have timestamps, use database order)
    // Messages are already in database order from the query

    // Get canonical path
    let canonical_path = get_canonical_path(
        PROVIDER_ID,
        Some(session.project_name()),
        &session.session_id,
    )?;

    // Write canonical JSONL
    write_canonical_file(&canonical_path, &canonical_messages)?;

    // Get file size
    let file_size = fs::metadata(&canonical_path)?.len();

    // Publish event
    let payload = SessionEventPayload::SessionChanged {
        session_id: session.session_id.clone(),
        project_name: session.project_name().to_string(),
        file_path: canonical_path.to_string_lossy().to_string(),
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
            timestamp: Utc::now().to_rfc3339(),
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
