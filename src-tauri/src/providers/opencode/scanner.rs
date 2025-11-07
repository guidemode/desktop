//! OpenCode session scanner - discovers and parses OpenCode sessions from ~/.local/share/opencode/storage/

use crate::logging::{log_info, log_warn};
use crate::providers::common::SessionInfo;
use std::fs;
use std::path::Path;

/// Scan all OpenCode sessions from the base path
pub fn scan_sessions_filtered(
    base_path: &Path,
    selected_projects: Option<&[String]>,
) -> Result<Vec<SessionInfo>, String> {
    // Import the OpenCode parser
    use super::parser::OpenCodeParser;

    let storage_path = base_path.join("storage");
    if !storage_path.exists() {
        return Ok(Vec::new());
    }

    let parser = OpenCodeParser::new(storage_path);
    let mut sessions = Vec::new();

    // Get all projects first
    let projects = parser
        .get_all_projects()
        .map_err(|e| format!("Failed to get OpenCode projects: {}", e))?;

    for project in projects {
        // Extract project name from worktree path
        let project_name = Path::new(&project.worktree)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Skip projects not in the selected list
        if let Some(selected) = selected_projects {
            if !selected.contains(&project_name) {
                continue;
            }
        }

        // Get all sessions for this project
        let session_ids = parser
            .get_sessions_for_project(&project.id)
            .map_err(|e| format!("Failed to get sessions for project {}: {}", project.id, e))?;

        for session_id in session_ids {
            match parse_opencode_session(&parser, &session_id, &project) {
                Ok(session_info) => sessions.push(session_info),
                Err(e) => {
                    if let Err(log_err) = log_warn(
                        "opencode",
                        &format!("Failed to parse OpenCode session {}: {}", session_id, e),
                    ) {
                        eprintln!("Logging error: {}", log_err);
                    }
                }
            }
        }
    }

    if let Err(e) = log_info(
        "opencode",
        &format!("📊 Found {} OpenCode sessions", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    Ok(sessions)
}

fn parse_opencode_session(
    parser: &super::parser::OpenCodeParser,
    session_id: &str,
    _project: &super::parser::OpenCodeProject,
) -> Result<SessionInfo, String> {
    use super::super::common::{extract_cwd_from_canonical_content, get_canonical_path};
    use super::converter::convert_opencode_jsonl_to_canonical;

    // Parse the session using the OpenCode parser
    // This aggregates session/message/part files into OpenCode JSONL format
    let parsed_session = parser
        .parse_session(session_id)
        .map_err(|e| format!("Failed to parse session with OpenCode parser: {}", e))?;

    // Convert aggregated OpenCode JSONL to canonical format
    let canonical_jsonl = convert_opencode_jsonl_to_canonical(&parsed_session.jsonl_content)
        .map_err(|e| format!("Failed to convert OpenCode JSONL to canonical: {}", e))?;

    // Extract CWD from canonical content for project organization
    let cwd = extract_cwd_from_canonical_content(&canonical_jsonl);

    // Get project-organized canonical path
    let cached_file_path = get_canonical_path("opencode", cwd.as_deref(), session_id)
        .map_err(|e| format!("Failed to get canonical path: {}", e))?;

    // Write canonical JSONL to project-organized path
    fs::write(&cached_file_path, &canonical_jsonl)
        .map_err(|e| format!("Failed to write cached JSONL: {}", e))?;

    let file_name = format!("{}.jsonl", session_id);

    let file_size = canonical_jsonl.len() as u64;

    Ok(SessionInfo {
        provider: "opencode".to_string(),
        project_name: parsed_session.project_name,
        session_id: parsed_session.session_id,
        file_path: cached_file_path,
        file_name,
        session_start_time: parsed_session.session_start_time,
        session_end_time: parsed_session.session_end_time,
        duration_ms: parsed_session.duration_ms,
        file_size,
        content: None, // Now using cached file, not in-memory content
        cwd: parsed_session.cwd,
        project_hash: None, // Not used for OpenCode
    })
}
