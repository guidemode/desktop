//! GitHub Copilot session scanner - discovers and parses Copilot sessions from ~/.copilot/session-state/

use crate::logging::{log_info, log_warn};
use crate::providers::common::SessionInfo;
use std::fs;
use std::path::Path;

/// Scan all GitHub Copilot sessions from the base path
pub fn scan_sessions_filtered(
    base_path: &Path,
    selected_projects: Option<&[String]>,
) -> Result<Vec<SessionInfo>, String> {
    // Copilot uses ~/.copilot/session-state/{uuid}.jsonl
    let session_dir = base_path.join("session-state");
    if !session_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = fs::read_dir(&session_dir)
        .map_err(|e| format!("Failed to read Copilot session directory: {}", e))?;

    for entry in entries.flatten() {
        let file_path = entry.path();

        // Only process JSONL session files (end with ".jsonl")
        if file_path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) {
                // Skip hidden files
                if !file_name.starts_with('.') {
                    match parse_copilot_session(&file_path, selected_projects) {
                        Ok(Some(session_info)) => {
                            sessions.push(session_info);
                        }
                        Ok(None) => {
                            // Session filtered out - skipped
                        }
                        Err(e) => {
                            if let Err(log_err) = log_warn(
                                "github-copilot",
                                &format!(
                                    "Failed to parse Copilot session {}: {}",
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

    if let Err(e) = log_info(
        "github-copilot",
        &format!("📊 Found {} GitHub Copilot sessions", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    Ok(sessions)
}

fn parse_copilot_session(
    file_path: &Path,
    selected_projects: Option<&[String]>,
) -> Result<Option<SessionInfo>, String> {
    use super::parser::CopilotParser;
    use super::super::common::get_canonical_path;

    // Use CopilotParser to parse the new JSONL event format
    let storage_path = file_path
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Invalid file path")?;

    let parser = CopilotParser::new(storage_path.to_path_buf());
    let parsed = parser.parse_session(file_path)?;

    // Filter projects BEFORE processing/caching
    if let Some(selected) = selected_projects {
        if !selected.contains(&parsed.project_name) {
            return Ok(None); // Skip this session
        }
    }

    // Write canonical format to project-organized path
    let cache_path = get_canonical_path("github-copilot", parsed.cwd.as_deref(), &parsed.session_id)
        .map_err(|e| format!("Failed to get canonical path: {}", e))?;

    fs::write(&cache_path, &parsed.jsonl_content)
        .map_err(|e| format!("Failed to write canonical cache file: {}", e))?;

    // Get file size of canonical cache file
    let file_size = fs::metadata(&cache_path).map(|m| m.len()).unwrap_or(0);
    let file_name = cache_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    Ok(Some(SessionInfo {
        provider: "github-copilot".to_string(),
        project_name: parsed.project_name,
        session_id: parsed.session_id,
        file_path: cache_path, // Use canonical cache path, not source path
        file_name,
        session_start_time: parsed.session_start_time,
        session_end_time: parsed.session_end_time,
        duration_ms: parsed.duration_ms,
        file_size,
        content: None,
        cwd: parsed.cwd,
        project_hash: None,
    }))
}
