use crate::logging::{log_debug, log_info, log_warn};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::fs;
use std::path::{Path, PathBuf};

/// Type alias for timing data tuple returned from JSONL parsing
type TimingData = (
    Option<DateTime<Utc>>, // start_time
    Option<DateTime<Utc>>, // end_time
    Option<i64>,           // duration_ms
);

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
    pub project_hash: Option<String>, // Project hash (for Gemini Code - SHA256 of CWD)
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
        "gemini-code" => scan_gemini_sessions(base_path),
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
        project_hash: None, // Not used for Claude Code
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
    use std::fs;

    // Parse the session using the OpenCode parser
    let parsed_session = parser
        .parse_session(session_id)
        .map_err(|e| format!("Failed to parse session with OpenCode parser: {}", e))?;

    // Create cache directory if it doesn't exist
    let cache_dir = dirs::home_dir()
        .ok_or("Could not find home directory")?
        .join(".guideai")
        .join("cache")
        .join("opencode");

    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;

    // Write virtual JSONL to cache (same as watcher does)
    let file_name = format!("{}.jsonl", session_id);
    let cached_file_path = cache_dir.join(&file_name);

    fs::write(&cached_file_path, &parsed_session.jsonl_content)
        .map_err(|e| format!("Failed to write cached JSONL: {}", e))?;

    let file_size = parsed_session.jsonl_content.len() as u64;

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
        project_hash: None, // Not used for Codex
    })
}

fn parse_copilot_session(file_path: &Path) -> Result<SessionInfo, String> {
    use super::copilot_parser::{detect_project_and_cwd_from_timeline, load_copilot_config, CopilotSession};
    use super::copilot_snapshot::SnapshotManager;

    // Read and parse the SOURCE Copilot JSON file
    let content =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Parse the Copilot session JSON (with timeline)
    let copilot_session: CopilotSession = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse Copilot session JSON: {}", e))?;

    let timeline = &copilot_session.timeline;

    // Skip if timeline is empty
    if timeline.is_empty() {
        return Err("Session has empty timeline".to_string());
    }

    // Detect project name and cwd from timeline entries (same as watcher)
    let (project_name, project_cwd) = match load_copilot_config() {
        Ok(config) => {
            if !config.trusted_folders.is_empty() {
                if let Some((name, cwd)) =
                    detect_project_and_cwd_from_timeline(timeline, &config.trusted_folders)
                {
                    (name, Some(cwd))
                } else {
                    ("copilot-sessions".to_string(), None)
                }
            } else {
                ("copilot-sessions".to_string(), None)
            }
        }
        Err(_) => ("copilot-sessions".to_string(), None),
    };

    // Create snapshot manager (same as watcher)
    let snapshot_manager = SnapshotManager::new()
        .map_err(|e| format!("Failed to create snapshot manager: {}", e))?;

    // Load metadata (with file lock)
    let (mut metadata, lock_file) = snapshot_manager
        .load_metadata_locked()
        .map_err(|e| format!("Failed to load metadata: {}", e))?;

    // Get or create snapshot (same logic as watcher)
    let snapshot_id = snapshot_manager
        .get_or_create_session(
            &mut metadata,
            file_path,
            &copilot_session.session_id,
            &copilot_session.start_time,
            timeline,
            project_cwd.as_deref(),
        )
        .map_err(|e| format!("Failed to create snapshot: {}", e))?;

    // Save metadata
    snapshot_manager
        .save_metadata_atomic(&metadata, lock_file)
        .map_err(|e| format!("Failed to save metadata: {}", e))?;

    // Get snapshot path
    let snapshot_path = snapshot_manager.get_snapshot_path(snapshot_id);
    let file_size = fs::metadata(&snapshot_path).map(|m| m.len()).unwrap_or(0);

    let file_name = format!("{}.jsonl", snapshot_id);

    // Extract timing from the snapshot JSONL (same as db_helpers does)
    let (session_start_time, session_end_time, duration_ms) =
        extract_timing_from_jsonl(&snapshot_path)?;

    Ok(SessionInfo {
        provider: "github-copilot".to_string(),
        project_name,
        session_id: snapshot_id.to_string(), // Use snapshot UUID, not source session ID
        file_path: snapshot_path, // Use snapshot path, not source path
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None,
        cwd: project_cwd,
        project_hash: None,
    })
}

// Helper to extract timing from JSONL (same logic as db_helpers)
fn extract_timing_from_jsonl(file_path: &Path) -> Result<TimingData, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read snapshot file: {}", e))?;

    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return Ok((None, None, None));
    }

    // Find first line with timestamp
    let session_start_time = lines.iter().find_map(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|entry| {
                entry
                    .get("timestamp")
                    .and_then(|ts| ts.as_str())
                    .and_then(|ts_str| {
                        DateTime::parse_from_rfc3339(ts_str)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
            })
    });

    // Find last line with timestamp
    let session_end_time = lines.iter().rev().find_map(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|entry| {
                entry
                    .get("timestamp")
                    .and_then(|ts| ts.as_str())
                    .and_then(|ts_str| {
                        DateTime::parse_from_rfc3339(ts_str)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
            })
    });

    // Calculate duration
    let duration_ms = match (session_start_time, session_end_time) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
        _ => None,
    };

    Ok((session_start_time, session_end_time, duration_ms))
}

fn scan_gemini_sessions(base_path: &Path) -> Result<Vec<SessionInfo>, String> {
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
    use super::gemini_parser::GeminiSession;
    use super::common::extract_session_id_from_filename;

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

    // Create cache directory for JSONL files (similar to OpenCode)
    let cache_dir = dirs::home_dir()
        .ok_or("Could not find home directory")?
        .join(".guideai")
        .join("cache")
        .join("gemini-code");

    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;

    // Try to extract CWD from message content using shared function
    let cwd = extract_cwd_from_gemini_session(&session);

    // Convert to JSONL format and cache it (with CWD enrichment)
    // Use the extracted session_id from filename, not from JSON
    let file_name = format!("{}.jsonl", session_id);
    let cached_file_path = cache_dir.join(&file_name);

    let jsonl_content = session
        .to_jsonl(cwd.as_deref())
        .map_err(|e| format!("Failed to convert to JSONL: {}", e))?;

    fs::write(&cached_file_path, &jsonl_content)
        .map_err(|e| format!("Failed to write cached JSONL: {}", e))?;

    let file_size = jsonl_content.len() as u64;

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
fn extract_cwd_from_gemini_session(session: &super::gemini_parser::GeminiSession) -> Option<String> {
    use super::gemini::infer_cwd_from_session;
    infer_cwd_from_session(session, &session.project_hash)
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
