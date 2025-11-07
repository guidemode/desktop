use crate::config::ProjectInfo;
use crate::logging::{log_debug, log_info, log_warn};
use crate::providers::common::SessionInfo;
use crate::providers::sort_projects_by_modified;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use shellexpand::tilde;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct ClaudeLogEntry {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    timestamp: Option<String>,
}

pub fn scan_projects(home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    let expanded = tilde(home_directory);
    let primary_base = PathBuf::from(expanded.into_owned());

    // Build candidate paths with fallbacks for cross-platform support
    let mut base_candidates = Vec::new();

    // Add the primary path
    base_candidates.push(primary_base.clone());

    // Add platform-specific fallbacks
    if let Some(home_dir) = dirs::home_dir() {
        // Standard home directory path
        base_candidates.push(home_dir.join(".claude"));
    }

    // Find first existing base path
    let base_path = base_candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| format!("Claude home directory not found. Tried: {}", home_directory))?;

    let projects_path = base_path.join("projects");
    if !projects_path.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&projects_path)
        .map_err(|e| format!("Failed to read projects directory: {}", e))?;

    let mut projects = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        let Ok(modified) = metadata.modified() else {
            continue;
        };

        let modified: DateTime<Utc> = modified.into();
        projects.push((
            modified,
            ProjectInfo {
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
                last_modified: modified.to_rfc3339(),
            },
        ));
    }

    Ok(sort_projects_by_modified(projects))
}

/// Scan all Claude Code sessions from the base path
pub fn scan_sessions_filtered(
    base_path: &Path,
    selected_projects: Option<&[String]>,
) -> Result<Vec<SessionInfo>, String> {
    let projects_path = base_path.join("projects");
    if !projects_path.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = fs::read_dir(&projects_path)
        .map_err(|e| format!("Failed to read Claude projects directory: {}", e))?;

    for entry in entries.flatten() {
        let project_path = entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let Some(project_name) = project_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // Skip projects not in the selected list
        if let Some(selected) = selected_projects {
            if !selected.contains(&project_name.to_string()) {
                continue;
            }
        }

        // Find all .jsonl files in this project
        if let Ok(project_entries) = fs::read_dir(&project_path) {
            for project_entry in project_entries.flatten() {
                let file_path = project_entry.path();
                if file_path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                    // Skip agent files - they will be merged when processing the main session
                    if let Some(filename) = file_path.file_name().and_then(|n| n.to_str()) {
                        use super::super::common::is_agent_file;
                        if is_agent_file(filename) {
                            continue;
                        }
                    }

                    match parse_claude_session(&file_path, project_name) {
                        Ok(mut session_info) => {
                            session_info.provider = "claude-code".to_string();
                            sessions.push(session_info);
                        }
                        Err(e) => {
                            if let Err(log_err) = log_warn(
                                "claude-code",
                                &format!("Failed to parse session {}: {}", file_path.display(), e),
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
        "claude-code",
        &format!("📊 Found {} Claude Code sessions", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    Ok(sessions)
}

fn extract_cwd_from_claude_session(content: &str) -> Option<String> {
    // Look through the first 50 lines for a cwd field
    for line in content.lines().take(50) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(cwd) = value.get("cwd").and_then(|v| v.as_str()) {
                return Some(cwd.to_string());
            }
        }
    }
    None
}

fn parse_claude_session(file_path: &Path, project_name: &str) -> Result<SessionInfo, String> {
    let content =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return Err("File is empty".to_string());
    }

    // Parse first line for session start
    let first_entry: ClaudeLogEntry =
        serde_json::from_str(lines[0]).map_err(|e| format!("Failed to parse first line: {}", e))?;

    // Parse last line for session end
    let last_entry: ClaudeLogEntry = serde_json::from_str(lines[lines.len() - 1])
        .map_err(|e| format!("Failed to parse last line: {}", e))?;

    // Extract session ID (prefer from first entry, fallback to filename)
    let session_id = first_entry
        .session_id
        .or(last_entry.session_id)
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|s| s.to_string())
        })
        .ok_or("Cannot determine session ID")?;

    // Parse timestamps
    let session_start_time = first_entry
        .timestamp
        .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let session_end_time = last_entry
        .timestamp
        .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Calculate duration
    let duration_ms = match (session_start_time, session_end_time) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
        _ => None,
    };

    // Get file size (from original source file, not used after copying to canonical cache)
    let _file_size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.jsonl")
        .to_string();

    // Try to extract CWD from session content (look for cwd in early entries)
    let cwd = extract_cwd_from_claude_session(&content);

    if cwd.is_none() {
        if let Err(e) = log_debug(
            "claude-code",
            &format!(
                "No CWD found in session {} (file: {})",
                session_id,
                file_path.display()
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }
    } else if let Err(e) = log_debug(
        "claude-code",
        &format!("✓ Extracted CWD from session {}: {:?}", session_id, cwd),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Convert to canonical format using shared conversion function
    // This will:
    // 1. Filter out system events (file-history-snapshot, summary, etc.)
    // 2. Add provider field
    // 3. Fix empty tool_result content
    // 4. Merge agent sidechain files
    use super::converter_utils::convert_to_canonical_file;

    let cache_path = convert_to_canonical_file(file_path, &session_id, cwd.as_deref())
        .map_err(|e| format!("Failed to convert session: {}", e))?;

    // Update file size from cache file
    let file_size = fs::metadata(&cache_path).map(|m| m.len()).unwrap_or(0);

    // IMPORTANT: Always use the Claude Code folder name for filtering
    // The real project name will be derived from CWD later during upload
    // This ensures filtering works correctly with user-selected projects

    Ok(SessionInfo {
        provider: "claude-code".to_string(),
        project_name: project_name.to_string(), // Use Claude Code folder name for filtering
        session_id,
        file_path: cache_path, // Use canonical cache path
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None,      // Claude Code sessions use files directly
        cwd,                // CWD will be used to derive real project name during upload
        project_hash: None, // Not used for Claude Code
    })
}
