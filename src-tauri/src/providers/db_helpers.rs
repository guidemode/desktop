use crate::database::{insert_session, session_exists, update_session};
use crate::logging::{log_debug, log_info, log_warn};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Insert or update a session in the local database immediately (called by all provider watchers)
pub fn insert_session_immediately(
    provider_id: &str,
    project_name: &str,
    session_id: &str,
    file_path: &PathBuf,
    file_size: u64,
    file_hash: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.jsonl");

    // Extract CWD from file
    let cwd = extract_cwd_from_file(provider_id, file_path);

    // Check if already exists
    if session_exists(session_id, file_name)? {
        // Update existing session with new file size and timestamp
        let (start_time, end_time, _duration) = match extract_session_timing(provider_id, file_path)
        {
            Ok(timing) => timing,
            Err(e) => {
                let _ = log_warn(
                    provider_id,
                    &format!("⚠ Could not extract session timing for update: {} - will update without timing data", e)
                );
                (None, None, None)
            }
        };

        update_session(
            session_id,
            file_name,
            file_size,
            file_hash.as_deref(),
            start_time,
            end_time,
            cwd.as_deref(),
        )?;

        // Also link project for existing sessions if CWD is available
        if let Some(ref cwd_path) = cwd {
            match crate::project_metadata::extract_project_metadata(cwd_path) {
                Ok(metadata) => {
                    match crate::database::insert_or_get_project(
                        &metadata.project_name,
                        metadata.git_remote_url.as_deref(),
                        &metadata.cwd,
                        &metadata.detected_project_type,
                    ) {
                        Ok(project_id) => {
                            if let Err(e) =
                                crate::database::attach_session_to_project(session_id, &project_id)
                            {
                                let _ = log_debug(
                                    provider_id,
                                    &format!(
                                        "⚠ Failed to attach session to project during update: {}",
                                        e
                                    ),
                                );
                            }
                        }
                        Err(e) => {
                            let _ = log_debug(
                                provider_id,
                                &format!("⚠ Failed to insert/get project during update: {}", e),
                            );
                        }
                    }
                }
                Err(_) => {
                    // Silently skip - project metadata not available
                }
            }
        }

        let _ = log_debug(
            provider_id,
            &format!("↻ Session {} updated in database", session_id),
        );
        return Ok(());
    }

    // Parse session timing from file
    let (start_time, end_time, duration) = match extract_session_timing(provider_id, file_path) {
        Ok(timing) => timing,
        Err(e) => {
            let _ = log_warn(
                provider_id,
                &format!("⚠ Could not extract session timing: {} - will save session without timing data", e)
            );
            (None, None, None)
        }
    };

    // Insert into database
    insert_session(
        provider_id,
        project_name,
        session_id,
        file_name,
        &file_path.to_string_lossy(),
        file_size,
        file_hash.as_deref(),
        start_time,
        end_time,
        duration,
        cwd.as_deref(),
    )?;

    // Extract and link project if CWD is available
    if let Some(ref cwd_path) = cwd {
        match crate::project_metadata::extract_project_metadata(cwd_path) {
            Ok(metadata) => {
                // Insert or update project
                match crate::database::insert_or_get_project(
                    &metadata.project_name,
                    metadata.git_remote_url.as_deref(),
                    &metadata.cwd,
                    &metadata.detected_project_type,
                ) {
                    Ok(project_id) => {
                        // Attach session to project
                        if let Err(e) =
                            crate::database::attach_session_to_project(session_id, &project_id)
                        {
                            let _ = log_warn(
                                provider_id,
                                &format!("⚠ Failed to attach session to project: {}", e),
                            );
                        } else {
                            let _ = log_debug(
                                provider_id,
                                &format!(
                                    "📁 Session {} linked to project {} ({})",
                                    session_id,
                                    metadata.project_name,
                                    metadata.detected_project_type
                                ),
                            );
                        }
                    }
                    Err(e) => {
                        let _ = log_warn(
                            provider_id,
                            &format!("⚠ Failed to insert/get project: {}", e),
                        );
                    }
                }
            }
            Err(e) => {
                let _ = log_debug(
                    provider_id,
                    &format!(
                        "⚠ Could not extract project metadata from {}: {}",
                        cwd_path, e
                    ),
                );
            }
        }
    }

    let timing_info = match (start_time, end_time, duration) {
        (Some(start), Some(end), Some(dur)) => format!(
            " | Start: {}, End: {}, Duration: {}ms",
            start.format("%H:%M:%S"),
            end.format("%H:%M:%S"),
            dur
        ),
        (Some(start), None, None) => format!(" | Start: {}, End: (none)", start.format("%H:%M:%S")),
        (None, Some(end), None) => format!(" | Start: (none), End: {}", end.format("%H:%M:%S")),
        _ => " | No timing data extracted".to_string(),
    };

    let _ = log_info(
        provider_id,
        &format!(
            "💾 Session {} saved to local database{}",
            session_id, timing_info
        ),
    );

    Ok(())
}

/// Extract session timing from JSONL file (works for all providers)
/// Extract timing information from session file (start time, end time, duration)
/// All providers now use JSONL format (including github-copilot snapshots)
fn extract_session_timing(
    _provider_id: &str,
    file_path: &PathBuf,
) -> Result<
    (Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<i64>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    use std::fs;

    // Read JSONL and extract timestamps
    let content = fs::read_to_string(file_path).map_err(|e| {
        let _ = log_warn(
            "database",
            &format!("⚠ Failed to read file for timing extraction: {}", e),
        );
        e
    })?;

    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        let _ = log_warn(
            "database",
            "⚠ No lines found in file for timing extraction",
        );
        return Ok((None, None, None));
    }

    // Find first line with a valid timestamp (scan from start)
    let session_start_time = lines.iter().find_map(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|entry| {
                entry
                    .get("timestamp")
                    .and_then(|ts| ts.as_str())
                    .and_then(|ts_str| {
                        DateTime::parse_from_rfc3339(ts_str).ok().map(|dt| dt.with_timezone(&Utc))
                    })
            })
    });

    // Find last line with a valid timestamp (scan from end)
    let session_end_time = lines.iter().rev().find_map(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|entry| {
                entry
                    .get("timestamp")
                    .and_then(|ts| ts.as_str())
                    .and_then(|ts_str| {
                        DateTime::parse_from_rfc3339(ts_str).ok().map(|dt| dt.with_timezone(&Utc))
                    })
            })
    });

    // Calculate duration
    let duration_ms = match (session_start_time, session_end_time) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
        (Some(_), None) => None, // Session still active
        (None, Some(_)) => {
            // Unusual: has end but no start
            let _ = log_warn(
                "database",
                "⚠️  Session has end time but no start time",
            );
            None
        }
        (None, None) => None, // No timestamps found
    };

    Ok((session_start_time, session_end_time, duration_ms))
}

/// Extract CWD from session file (provider-specific logic)
fn extract_cwd_from_file(provider_id: &str, file_path: &PathBuf) -> Option<String> {
    use std::fs;

    let content = fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return None;
    }

    // Different providers store CWD differently
    match provider_id {
        "claude-code" | "codex" => {
            // Claude Code and Codex: Look for direct cwd field or payload.cwd field
            for line in lines.iter().take(50) {
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                    // Check for direct cwd field
                    if let Some(cwd) = entry.get("cwd").and_then(|v| v.as_str()) {
                        return Some(cwd.to_string());
                    }
                    // Check for payload.cwd field
                    if let Some(cwd) = entry
                        .get("payload")
                        .and_then(|p| p.get("cwd"))
                        .and_then(|v| v.as_str())
                    {
                        return Some(cwd.to_string());
                    }
                }
            }
        }
        "github-copilot" => {
            // GitHub Copilot: Look for direct cwd field added by our snapshot manager
            // Since we add cwd to every timeline entry, we only need to check the first 50 lines
            for line in lines.iter().take(50) {
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                    // Check for direct cwd field
                    if let Some(cwd) = entry.get("cwd").and_then(|v| v.as_str()) {
                        return Some(cwd.to_string());
                    }
                }
            }
        }
        "opencode" => {
            // OpenCode: Look for cwd field in virtual JSONL
            for line in lines {
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(cwd) = entry.get("cwd").and_then(|v| v.as_str()) {
                        return Some(cwd.to_string());
                    }
                }
            }
        }
        "gemini-code" => {
            // Gemini Code: Look for cwd field in JSONL (added during conversion)
            for line in lines.iter().take(50) {
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(cwd) = entry.get("cwd").and_then(|v| v.as_str()) {
                        return Some(cwd.to_string());
                    }
                }
            }
        }
        _ => {}
    }

    let _ = log_debug(
        "database",
        &format!(
            "⚠ No CWD found in session file for provider {}",
            provider_id
        ),
    );
    None
}
