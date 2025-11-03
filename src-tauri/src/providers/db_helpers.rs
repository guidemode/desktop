use crate::database::{insert_session, update_session};
use crate::logging::{log_debug, log_info, log_warn};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Type alias for timing data tuple returned from JSONL parsing
type TimingResult = Result<
    (Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<i64>),
    Box<dyn std::error::Error + Send + Sync>,
>;

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

    // Extract git info if CWD is available
    let (git_branch, git_commit) = if let Some(ref cwd_path) = cwd {
        let branch = crate::project_metadata::extract_git_branch(cwd_path);
        let commit = crate::project_metadata::extract_git_commit_hash(cwd_path);
        (branch, commit)
    } else {
        (None, None)
    };

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

    // Try to insert first (optimistic path for new sessions)
    // If it fails due to unique constraint, update instead
    let insert_result = insert_session(
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
        git_branch.as_deref(),
        git_commit.as_deref(), // first_commit_hash
        git_commit.as_deref(), // latest_commit_hash (same as first at creation)
    );

    // Handle insert result - if unique constraint violation, update instead
    match insert_result {
        Ok(_) => {
            // Insert succeeded - this is a new session
            let timing_info = match (start_time, end_time, duration) {
                (Some(start), Some(end), Some(dur)) => format!(
                    " | Start: {}, End: {}, Duration: {}ms",
                    start.format("%H:%M:%S"),
                    end.format("%H:%M:%S"),
                    dur
                ),
                (Some(start), None, None) => {
                    format!(" | Start: {}, End: (none)", start.format("%H:%M:%S"))
                }
                (None, Some(end), None) => {
                    format!(" | Start: (none), End: {}", end.format("%H:%M:%S"))
                }
                _ => " | No timing data extracted".to_string(),
            };

            let _ = log_info(
                provider_id,
                &format!(
                    "💾 Session {} saved to local database{}",
                    session_id, timing_info
                ),
            );
        }
        Err(e) => {
            // Check if this is a unique constraint violation
            let is_constraint_violation = e.to_string().contains("UNIQUE constraint");

            if is_constraint_violation {
                // Session already exists, update it instead
                let _ = log_debug(
                    provider_id,
                    &format!("Session {} already exists, updating instead", session_id),
                );

                update_session(
                    session_id,
                    file_name,
                    file_size,
                    file_hash.as_deref(),
                    start_time,
                    end_time,
                    cwd.as_deref(),
                    git_branch.as_deref(),
                    git_commit.as_deref(), // latest_commit_hash (updates on each change)
                )?;

                let _ = log_debug(
                    provider_id,
                    &format!("↻ Session {} updated in database", session_id),
                );
            } else {
                // Some other error, propagate it
                return Err(Box::new(e));
            }
        }
    }

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
                            // Update the project_name field to match the linked project
                            // This ensures the session displays the correct project name instead of fallback
                            if let Err(e) = crate::database::update_session_project_name(
                                session_id,
                                &metadata.project_name,
                            ) {
                                let _ = log_warn(
                                    provider_id,
                                    &format!("⚠ Failed to update session project_name: {}", e),
                                );
                            }

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

    Ok(())
}

/// Extract session timing from JSONL file (works for all providers)
/// Extract timing information from session file (start time, end time, duration)
/// All providers now use JSONL format (including github-copilot snapshots)
fn extract_session_timing(_provider_id: &str, file_path: &PathBuf) -> TimingResult {
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
        let _ = log_warn("database", "⚠ No lines found in file for timing extraction");
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
                        DateTime::parse_from_rfc3339(ts_str)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
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
                        DateTime::parse_from_rfc3339(ts_str)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
            })
    });

    // Calculate duration
    let duration_ms = match (session_start_time, session_end_time) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
        (Some(_), None) => None, // Session still active
        (None, Some(_)) => {
            // Unusual: has end but no start
            let _ = log_warn("database", "⚠️  Session has end time but no start time");
            None
        }
        (None, None) => None, // No timestamps found
    };

    Ok((session_start_time, session_end_time, duration_ms))
}

/// Extract CWD from session file (provider-specific logic)
fn extract_cwd_from_file(_provider_id: &str, file_path: &PathBuf) -> Option<String> {
    use std::fs;

    // Read file content
    let content = fs::read_to_string(file_path).ok()?;

    // Use shared utility to extract CWD from canonical content
    // (All providers now use canonical format with cwd at top level)
    crate::providers::common::canonical_path::extract_cwd_from_canonical_content(&content)
}
