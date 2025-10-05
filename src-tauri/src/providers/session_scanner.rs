use crate::logging::{log_debug, log_info, log_warn};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub provider: String,
    pub project_name: String,
    pub session_id: String,
    pub file_path: PathBuf,
    pub file_name: String,
    pub session_start_time: Option<DateTime<Utc>>,
    pub session_end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub file_size: u64,
    pub content: Option<String>, // For OpenCode sessions with in-memory content
    pub cwd: Option<String>,     // Working directory for the session
}

#[derive(Debug, Deserialize)]
struct ClaudeLogEntry {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexLogEntry {
    timestamp: Option<String>,
    payload: Option<CodexPayload>,
}

#[derive(Debug, Deserialize)]
struct CodexPayload {
    id: Option<String>,
    cwd: Option<String>,
}

pub fn scan_all_sessions(
    provider_id: &str,
    home_directory: &str,
) -> Result<Vec<SessionInfo>, String> {
    let expanded = tilde(home_directory);
    let base_path = Path::new(expanded.as_ref());

    if !base_path.exists() {
        return Ok(Vec::new());
    }

    match provider_id {
        "claude-code" => scan_claude_sessions(base_path),
        "github-copilot" => scan_copilot_sessions(base_path),
        "opencode" => scan_opencode_sessions(base_path),
        "codex" => scan_codex_sessions(base_path),
        _ => Err(format!("Unsupported provider: {}", provider_id)),
    }
}

fn scan_claude_sessions(base_path: &Path) -> Result<Vec<SessionInfo>, String> {
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

        // Find all .jsonl files in this project
        if let Ok(project_entries) = fs::read_dir(&project_path) {
            for project_entry in project_entries.flatten() {
                let file_path = project_entry.path();
                if file_path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
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
        .or_else(|| last_entry.session_id)
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

    // Get file size
    let file_size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

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

    // IMPORTANT: Always use the Claude Code folder name for filtering
    // The real project name will be derived from CWD later during upload
    // This ensures filtering works correctly with user-selected projects

    Ok(SessionInfo {
        provider: "claude-code".to_string(),
        project_name: project_name.to_string(), // Use Claude Code folder name for filtering
        session_id,
        file_path: file_path.to_path_buf(),
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None, // Claude Code sessions use files directly
        cwd,           // CWD will be used to derive real project name during upload
    })
}

fn scan_opencode_sessions(base_path: &Path) -> Result<Vec<SessionInfo>, String> {
    // Import the OpenCode parser
    use super::opencode_parser::OpenCodeParser;

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
    parser: &super::opencode_parser::OpenCodeParser,
    session_id: &str,
    _project: &super::opencode_parser::OpenCodeProject,
) -> Result<SessionInfo, String> {
    // Parse the session using the OpenCode parser
    let parsed_session = parser
        .parse_session(session_id)
        .map_err(|e| format!("Failed to parse session with OpenCode parser: {}", e))?;

    // Create a temporary file path for the session (since we're generating in-memory content)
    let file_name = format!("{}.jsonl", session_id);
    let dummy_file_path = PathBuf::from(&file_name);

    Ok(SessionInfo {
        provider: "opencode".to_string(),
        project_name: parsed_session.project_name,
        session_id: parsed_session.session_id,
        file_path: dummy_file_path,
        file_name,
        session_start_time: parsed_session.session_start_time,
        session_end_time: parsed_session.session_end_time,
        duration_ms: parsed_session.duration_ms,
        file_size: parsed_session.jsonl_content.len() as u64,
        content: Some(parsed_session.jsonl_content), // OpenCode sessions have in-memory content
        cwd: parsed_session.cwd,                     // OpenCode sessions have CWD
    })
}

fn scan_codex_sessions(base_path: &Path) -> Result<Vec<SessionInfo>, String> {
    // Codex uses ~/.codex/sessions/YYYY/MM/DD/*.jsonl structure
    let sessions_path = base_path.join("sessions");
    if !sessions_path.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    // Recursively find all .jsonl files in the sessions directory
    fn find_session_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries = fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_session_files(&path, files)?;
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
        Ok(())
    }

    let mut session_files = Vec::new();
    find_session_files(&sessions_path, &mut session_files)?;

    for file_path in session_files {
        match parse_codex_session(&file_path) {
            Ok(session_info) => {
                sessions.push(session_info);
            }
            Err(e) => {
                if let Err(log_err) = log_warn(
                    "codex",
                    &format!(
                        "Failed to parse Codex session {}: {}",
                        file_path.display(),
                        e
                    ),
                ) {
                    eprintln!("Logging error: {}", log_err);
                }
            }
        }
    }

    if let Err(e) = log_info(
        "codex",
        &format!("📊 Found {} Codex sessions", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    Ok(sessions)
}

fn scan_copilot_sessions(base_path: &Path) -> Result<Vec<SessionInfo>, String> {
    // Copilot uses ~/.copilot/history-session-state/session_{uuid}_{timestamp}.json
    let session_dir = base_path.join("history-session-state");
    if !session_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    let entries = fs::read_dir(&session_dir)
        .map_err(|e| format!("Failed to read Copilot session directory: {}", e))?;

    for entry in entries.flatten() {
        let file_path = entry.path();

        // Only process session files (start with "session_" and end with ".json")
        if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) {
            if file_name.starts_with("session_")
                && file_path.extension().and_then(|ext| ext.to_str()) == Some("json")
            {
                match parse_copilot_session(&file_path) {
                    Ok(session_info) => {
                        sessions.push(session_info);
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

    if let Err(e) = log_info(
        "github-copilot",
        &format!("📊 Found {} GitHub Copilot sessions", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    Ok(sessions)
}

fn parse_codex_session(file_path: &Path) -> Result<SessionInfo, String> {
    let content =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return Err("File is empty".to_string());
    }

    // Parse first line for session metadata
    let first_entry: CodexLogEntry =
        serde_json::from_str(lines[0]).map_err(|e| format!("Failed to parse first line: {}", e))?;

    // Extract session info from first line metadata
    let payload = first_entry
        .payload
        .ok_or("No payload in session metadata")?;
    let session_id = payload.id.ok_or("No session ID in payload")?;
    let cwd = payload.cwd.ok_or("No cwd in payload")?;

    // Extract project name from cwd path
    let project_name = Path::new(&cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Parse session start time from first line
    let session_start_time = first_entry
        .timestamp
        .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Parse last line for session end time
    let last_entry: CodexLogEntry = serde_json::from_str(lines[lines.len() - 1])
        .map_err(|e| format!("Failed to parse last line: {}", e))?;

    let session_end_time = last_entry
        .timestamp
        .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Calculate duration
    let duration_ms = match (session_start_time, session_end_time) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
        _ => None,
    };

    // Get file size
    let file_size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.jsonl")
        .to_string();

    Ok(SessionInfo {
        provider: "codex".to_string(),
        project_name,
        session_id,
        file_path: file_path.to_path_buf(),
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None,  // Codex sessions use files directly
        cwd: Some(cwd), // Codex sessions have CWD from parsing
    })
}

#[derive(Debug, Deserialize)]
struct CopilotSessionFile {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "chatMessages")]
    chat_messages: Vec<serde_json::Value>,
}

fn parse_copilot_session(file_path: &Path) -> Result<SessionInfo, String> {
    let content =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Parse the Copilot session JSON
    let session: CopilotSessionFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse Copilot session JSON: {}", e))?;

    // Parse start time
    let session_start_time = DateTime::parse_from_rfc3339(&session.start_time)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));

    // Estimate end time from last message (Copilot doesn't store explicit end time)
    let session_end_time = session_start_time.map(|start| {
        // Add number of messages as a rough estimate (1 minute per message)
        start + chrono::Duration::minutes(session.chat_messages.len() as i64)
    });

    // Calculate duration
    let duration_ms = match (session_start_time, session_end_time) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
        _ => None,
    };

    // Get file size
    let file_size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.json")
        .to_string();

    // Extract session ID from filename if needed (format: session_{uuid}_{timestamp}.json)
    let session_id = file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|name| {
            // Remove "session_" prefix and everything after the last underscore (timestamp)
            if let Some(stripped) = name.strip_prefix("session_") {
                if let Some(last_underscore) = stripped.rfind('_') {
                    return Some(stripped[..last_underscore].to_string());
                }
            }
            None
        })
        .unwrap_or_else(|| session.session_id.clone());

    Ok(SessionInfo {
        provider: "github-copilot".to_string(),
        project_name: "copilot-sessions".to_string(), // Copilot doesn't have projects
        session_id,
        file_path: file_path.to_path_buf(),
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None, // Copilot sessions use files directly
        cwd: None,     // Copilot sessions don't have explicit CWD
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_claude_session() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test-session.jsonl");

        let content = r#"{"sessionId":"abc123","timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":{"role":"user","content":"Hello"}}
{"sessionId":"abc123","timestamp":"2025-01-01T10:30:00.000Z","type":"assistant","message":{"role":"assistant","content":"Hi there!"}}"#;

        fs::write(&file_path, content).unwrap();

        let result = parse_claude_session(&file_path, "test-project").unwrap();

        assert_eq!(result.session_id, "abc123");
        assert_eq!(result.project_name, "test-project");
        assert_eq!(result.provider, "claude-code");
        assert!(result.session_start_time.is_some());
        assert!(result.session_end_time.is_some());
        assert_eq!(result.duration_ms, Some(1800000)); // 30 minutes
        assert!(result.content.is_none()); // Claude Code sessions don't use in-memory content
    }

    // Note: test_parse_opencode_session removed because it requires the OpenCode parser
    // which needs a full storage structure. OpenCode sessions are tested through integration tests.

    #[test]
    fn test_parse_codex_session() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir
            .path()
            .join("rollout-2025-09-28T10-23-35-test.jsonl");

        let content = r#"{"timestamp":"2025-09-28T08:23:35.126Z","type":"session_meta","payload":{"id":"01998f6b-8fc9-7782-8d57-ca53fbfd057a","timestamp":"2025-09-28T08:23:35.113Z","cwd":"/Users/cliftonc/work/guideai","originator":"codex_cli_rs","cli_version":"0.42.0","instructions":null}}
{"timestamp":"2025-09-28T08:24:16.297Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Hello"}]}}"#;

        fs::write(&file_path, content).unwrap();

        let result = parse_codex_session(&file_path).unwrap();

        assert_eq!(result.session_id, "01998f6b-8fc9-7782-8d57-ca53fbfd057a");
        assert_eq!(result.project_name, "guideai");
        assert_eq!(result.provider, "codex");
        assert!(result.session_start_time.is_some());
        assert!(result.session_end_time.is_some());
        assert_eq!(result.duration_ms, Some(41171)); // ~41 seconds
        assert!(result.content.is_none()); // Codex sessions don't use in-memory content
    }
}
