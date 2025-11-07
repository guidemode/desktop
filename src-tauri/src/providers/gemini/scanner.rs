//! Gemini session scanner - discovers and parses Gemini sessions from ~/.gemini/tmp/

use crate::logging::{log_info, log_warn};
use crate::providers::common::SessionInfo;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

/// Scan all Gemini sessions from the base path
pub fn scan_sessions_filtered(
    base_path: &Path,
    selected_projects: Option<&[String]>,
) -> Result<Vec<SessionInfo>, String> {
    // Gemini uses ~/.gemini/tmp/{hash}/chats/session-*.json structure
    let tmp_path = base_path.join("tmp");
    if !tmp_path.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    // Recursively scan project hash directories
    let entries = fs::read_dir(&tmp_path)
        .map_err(|e| format!("Failed to read Gemini tmp directory: {}", e))?;

    for entry in entries.flatten() {
        let project_path = entry.path();

        // Skip the 'bin' directory
        if let Some(name) = project_path.file_name().and_then(|n| n.to_str()) {
            if name == "bin" {
                continue;
            }
        }

        if !project_path.is_dir() {
            continue;
        }

        // Get project hash (folder name)
        let project_hash = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        // Skip projects not in the selected list (Gemini uses hashes for filtering)
        if let Some(selected) = selected_projects {
            if let Some(ref hash) = project_hash {
                if !selected.contains(hash) {
                    continue;
                }
            } else {
                continue; // Skip if we can't determine hash
            }
        }

        let chats_path = project_path.join("chats");
        if !chats_path.exists() {
            continue;
        }

        // Scan all session files in the chats directory
        if let Ok(chat_entries) = fs::read_dir(&chats_path) {
            for chat_entry in chat_entries.flatten() {
                let file_path = chat_entry.path();

                // Only process session JSON files
                if let Some(filename) = file_path.file_name().and_then(|n| n.to_str()) {
                    if filename.starts_with("session-") && filename.ends_with(".json") {
                        match parse_gemini_session(&file_path) {
                            Ok(session_info) => {
                                sessions.push(session_info);
                            }
                            Err(e) => {
                                if let Err(log_err) = log_warn(
                                    "gemini-code",
                                    &format!(
                                        "Failed to parse Gemini session {}: {}",
                                        file_path.display(),
                                        e
                                    ),
                                ) {
                                    eprintln!("Logging error: {}", log_err);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Err(e) = log_info(
        "gemini-code",
        &format!("📊 Found {} Gemini sessions", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    Ok(sessions)
}

fn parse_gemini_session(file_path: &Path) -> Result<SessionInfo, String> {
    use super::super::common::extract_session_id_from_filename;
    use super::converter::convert_to_canonical_file;
    use super::parser::GeminiSession;

    let content =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Parse the Gemini session JSON
    let session: GeminiSession = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse Gemini session JSON: {}", e))?;

    // IMPORTANT: Use filename as session_id (not the sessionId field from JSON)
    // This matches the watcher behavior and ensures consistency
    let session_id = extract_session_id_from_filename(file_path);

    // Parse start time
    let session_start_time = DateTime::parse_from_rfc3339(&session.start_time)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));

    // Parse last updated time as end time
    let session_end_time = DateTime::parse_from_rfc3339(&session.last_updated)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));

    // Calculate duration
    let duration_ms = match (session_start_time, session_end_time) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
        _ => None,
    };

    // Convert to canonical format using shared function
    // This function handles:
    // - Inferring CWD from message content
    // - Converting to canonical format
    // - Serializing to JSONL
    // - Getting project-organized canonical path
    // - Writing to cache
    let cached_file_path = convert_to_canonical_file(file_path, &session_id)
        .map_err(|e| format!("Failed to convert to canonical format: {}", e))?;

    let file_name = format!("{}.jsonl", session_id);

    // Get file size from actual cached file (not in-memory string)
    // This ensures consistency with the watcher and prevents timing issues
    let file_size = fs::metadata(&cached_file_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Extract CWD from the cached file for project name determination
    let cwd = extract_cwd_from_gemini_session(&session);

    // Determine project name from CWD or use hash
    let project_name = if let Some(cwd_path) = &cwd {
        Path::new(cwd_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&session.project_hash)
            .to_string()
    } else {
        format!("gemini-{}", &session.project_hash[..8])
    };

    Ok(SessionInfo {
        provider: "gemini-code".to_string(),
        project_name,
        session_id, // Use filename-based ID, not session.session_id from JSON
        file_path: cached_file_path,
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None, // Now using cached file, not in-memory content
        cwd,
        project_hash: Some(session.project_hash), // Used for filtering Gemini sessions
    })
}

/// Extract CWD from Gemini session using shared extraction logic
fn extract_cwd_from_gemini_session(
    session: &super::parser::GeminiSession,
) -> Option<String> {
    use super::utils::infer_cwd_from_session;
    infer_cwd_from_session(session, &session.project_hash)
}
