use crate::database::{insert_session, update_session};
use crate::logging::{log_debug, log_info, log_warn};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Type alias for timing data tuple returned from JSONL parsing
type TimingResult = Result<
    (Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<i64>),
    Box<dyn std::error::Error + Send + Sync>,
>;

/// Helper function to query existing git data for a session
fn get_existing_git_data(
    session_id: &str,
) -> Option<(Option<String>, Option<String>, Option<String>)> {
    crate::database::with_connection_mut(|conn| {
        conn.query_row(
            "SELECT git_branch, first_commit_hash, latest_commit_hash FROM agent_sessions WHERE session_id = ?",
            rusqlite::params![session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
    }).ok()
}

/// Insert or update a session in the local database immediately (called by all provider watchers)
///
/// # Parameters
/// * `is_historical` - If true, preserves existing git data or sets to None for new sessions.
///   If false, captures current git state (normal behavior for live sessions).
pub fn insert_session_immediately(
    provider_id: &str,
    repository_name: &str,
    session_id: &str,
    file_path: &PathBuf,
    file_size: u64,
    file_hash: Option<String>,
    is_historical: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.jsonl");

    // Extract CWD from file
    let cwd = extract_cwd_from_file(provider_id, file_path);

    // Determine git info based on whether this is a historical scan
    let (git_branch, first_commit, latest_commit) = if is_historical {
        // For historical sessions, check if we already have git data in the database
        if let Some((existing_branch, existing_first_commit, existing_latest_commit)) =
            get_existing_git_data(session_id)
        {
            // Preserve existing git data from when session was live
            let _ = log_debug(
                provider_id,
                &format!(
                    "Preserving existing git data for historical session {}",
                    session_id
                ),
            );
            // Preserve both first and latest commit hashes from database
            (
                existing_branch,
                existing_first_commit,
                existing_latest_commit,
            )
        } else {
            // New historical session discovered - don't capture current git state (would be inaccurate)
            let _ = log_debug(
                provider_id,
                &format!(
                    "New historical session {}, not capturing git state",
                    session_id
                ),
            );
            (None, None, None)
        }
    } else {
        // Live session - capture current git state (existing behavior)
        if let Some(ref cwd_path) = cwd {
            let branch = crate::project_metadata::extract_git_branch(cwd_path);
            let commit = crate::project_metadata::extract_git_commit_hash(cwd_path);
            // For live sessions, first and latest are the same at creation
            (branch, commit.clone(), commit)
        } else {
            (None, None, None)
        }
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
        repository_name,
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
        first_commit.as_deref(),  // first_commit_hash
        latest_commit.as_deref(), // latest_commit_hash
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
                    &file_path.to_string_lossy(),
                    file_size,
                    file_hash.as_deref(),
                    start_time,
                    end_time,
                    cwd.as_deref(),
                    git_branch.as_deref(),
                    latest_commit.as_deref(), // latest_commit_hash
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

    // Extract and link repository if CWD is available
    if let Some(ref cwd_path) = cwd {
        match crate::project_metadata::extract_project_metadata(cwd_path) {
            Ok(metadata) => {
                // Insert or update repository
                match crate::database::insert_or_get_repository(
                    &metadata.project_name,
                    metadata.git_remote_url.as_deref(),
                    &metadata.cwd,
                    &metadata.detected_project_type,
                ) {
                    Ok(repository_id) => {
                        // Attach session to repository
                        if let Err(e) = crate::database::attach_session_to_repository(
                            session_id,
                            &repository_id,
                        ) {
                            let _ = log_warn(
                                provider_id,
                                &format!("⚠ Failed to attach session to repository: {}", e),
                            );
                        } else {
                            // Update the repository_name field to match the linked repository
                            // This ensures the session displays the correct repository name instead of fallback
                            if let Err(e) = crate::database::update_session_repository_name(
                                session_id,
                                &metadata.project_name,
                            ) {
                                let _ = log_warn(
                                    provider_id,
                                    &format!("⚠ Failed to update session repository_name: {}", e),
                                );
                            }

                            let _ = log_debug(
                                provider_id,
                                &format!(
                                    "📁 Session {} linked to repository {} ({})",
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
                            &format!("⚠ Failed to insert/get repository: {}", e),
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
