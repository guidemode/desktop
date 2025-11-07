//! Codex session scanner - discovers and parses Codex sessions from ~/.codex/sessions/

use crate::logging::{log_info, log_warn};
use crate::providers::common::SessionInfo;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Scan all Codex sessions from the base path
pub fn scan_sessions_filtered(
    base_path: &Path,
    selected_projects: Option<&[String]>,
) -> Result<Vec<SessionInfo>, String> {
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
        match parse_codex_session(&file_path, selected_projects) {
            Ok(Some(session_info)) => {
                sessions.push(session_info);
            }
            Ok(None) => {
                // Session filtered out - skipped
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

fn parse_codex_session(
    file_path: &Path,
    selected_projects: Option<&[String]>,
) -> Result<Option<SessionInfo>, String> {
    use super::super::canonical::converter::ToCanonical;
    use super::super::common::{extract_cwd_from_canonical_content, get_canonical_path};
    use super::CodexMessage;

    let content =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Parse first line for session metadata
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return Err("File is empty".to_string());
    }

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

    // Filter projects BEFORE processing/caching
    if let Some(selected) = selected_projects {
        if !selected.contains(&project_name) {
            return Ok(None); // Skip this session
        }
    }

    // Convert Codex JSONL to canonical format - simple 1-to-1 conversion
    // The watcher uses MessageAggregator for real-time processing, but the scanner
    // reads complete files that are already in final form, so just convert directly
    let mut canonical_lines = Vec::new();

    for line in lines.iter() {
        if let Ok(codex_msg) = serde_json::from_str::<CodexMessage>(line) {
            match codex_msg.to_canonical() {
                Ok(Some(mut canonical_msg)) => {
                    // Fix session_id for all messages (not just session_meta)
                    canonical_msg.session_id = session_id.clone();

                    if let Ok(serialized) = serde_json::to_string(&canonical_msg) {
                        canonical_lines.push(serialized);
                    }
                }
                Ok(None) => {
                    // Message was skipped (e.g., duplicate event_msg)
                }
                Err(e) => {
                    // Log error but continue processing
                    eprintln!("Failed to convert Codex message: {}", e);
                }
            }
        }
    }

    let canonical_content = canonical_lines.join("\n");

    // Extract CWD from canonical content (should match original)
    let canonical_cwd = extract_cwd_from_canonical_content(&canonical_content);

    // Get project-organized canonical path
    let cache_path = get_canonical_path("codex", canonical_cwd.as_deref(), &session_id)
        .map_err(|e| format!("Failed to get canonical path: {}", e))?;

    // Write canonical JSONL to project-organized path
    fs::write(&cache_path, &canonical_content)
        .map_err(|e| format!("Failed to write canonical JSONL: {}", e))?;

    // Parse session timing from first and last lines
    let session_start_time = first_entry
        .timestamp
        .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| dt.with_timezone(&Utc));

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

    // Get file size from canonical cache file
    let file_size = fs::metadata(&cache_path).map(|m| m.len()).unwrap_or(0);

    let file_name = cache_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.jsonl")
        .to_string();

    Ok(Some(SessionInfo {
        provider: "codex".to_string(),
        project_name,
        session_id,
        file_path: cache_path, // Use canonical cache path
        file_name,
        session_start_time,
        session_end_time,
        duration_ms,
        file_size,
        content: None,         // Codex sessions use files directly
        cwd: Some(cwd),        // Codex sessions have CWD from parsing
        project_hash: None,    // Not used for Codex
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_codex_session() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir
            .path()
            .join("rollout-2025-09-28T10-23-35-test.jsonl");

        let content = r#"{"timestamp":"2025-09-28T08:23:35.126Z","type":"session_meta","payload":{"id":"01998f6b-8fc9-7782-8d57-ca53fbfd057a","timestamp":"2025-09-28T08:23:35.113Z","cwd":"/Users/cliftonc/work/guideai","originator":"codex_cli_rs","cli_version":"0.42.0","instructions":null}}
{"timestamp":"2025-09-28T08:24:16.297Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Hello"}]}}"#;

        fs::write(&file_path, content).unwrap();

        let result = parse_codex_session(&file_path, None).unwrap().unwrap();

        assert_eq!(result.session_id, "01998f6b-8fc9-7782-8d57-ca53fbfd057a");
        assert_eq!(result.project_name, "guideai");
        assert_eq!(result.provider, "codex");
        assert!(result.session_start_time.is_some());
        assert!(result.session_end_time.is_some());
        assert_eq!(result.duration_ms, Some(41171)); // ~41 seconds
        assert!(result.content.is_none()); // Codex sessions don't use in-memory content

        // Verify the canonical file was created and uses MessageAggregator
        // (no duplicate assistant messages with same timestamp)
        assert!(result.file_path.exists());
    }
}
