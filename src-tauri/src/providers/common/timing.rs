//! Timing extraction utilities for session JSONL files
//!
//! This module provides functions to extract start time, end time, and duration
//! from canonical JSONL session files.

use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

/// Type alias for timing data tuple
///
/// Returns (start_time, end_time, duration_ms)
#[allow(dead_code)]
pub type TimingData = (
    Option<DateTime<Utc>>, // start_time
    Option<DateTime<Utc>>, // end_time
    Option<i64>,           // duration_ms
);

/// Extract timing information from a canonical JSONL file
///
/// Reads the JSONL file and extracts:
/// - First timestamp (session start)
/// - Last timestamp (session end)
/// - Duration in milliseconds (calculated from start and end)
///
/// # Arguments
///
/// * `file_path` - Path to the canonical JSONL session file
///
/// # Returns
///
/// Returns `Ok((start_time, end_time, duration_ms))` on success.
/// Any of the values may be `None` if not found in the file.
///
/// # Errors
///
/// Returns `Err` if the file cannot be read.
///
/// # Example
///
/// ```rust,ignore
/// use std::path::Path;
/// use crate::providers::common::timing::extract_timing_from_jsonl;
///
/// let file_path = Path::new("~/.guideai/sessions/claude-code/myproject/session.jsonl");
/// let (start, end, duration) = extract_timing_from_jsonl(file_path)?;
/// ```
#[allow(dead_code)]
pub fn extract_timing_from_jsonl(file_path: &Path) -> Result<TimingData, String> {
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
