//! Validation utilities for JSONL content and file checks.
//!
//! Ensures upload items meet quality requirements before processing.

use crate::logging::log_warn;
use serde_json::Value;

/// Validate that JSONL content contains at least one entry with a timestamp field
pub fn validate_jsonl_timestamps(content: &str) -> (bool, Option<String>) {
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return (
            false,
            Some("File is empty or contains only whitespace".to_string()),
        );
    }

    let mut has_valid_json = false;
    let mut parse_errors = 0;

    // Check if at least one line has a timestamp field
    for (index, line) in lines.iter().enumerate() {
        if let Ok(entry) = serde_json::from_str::<Value>(line) {
            has_valid_json = true;
            // Look for timestamp field (common across providers)
            if entry.get("timestamp").is_some() {
                return (true, None);
            }
        } else {
            parse_errors += 1;
            if index < 3 {
                // Log first few parse errors for debugging
                log_warn(
                    "upload-queue",
                    &format!(
                        "  Line {} failed to parse as JSON: {}",
                        index + 1,
                        &line[..line.len().min(100)]
                    ),
                )
                .unwrap_or_default();
            }
        }
    }

    if !has_valid_json {
        return (
            false,
            Some(format!(
                "No valid JSON lines found ({} parse errors)",
                parse_errors
            )),
        );
    }

    (
        false,
        Some(format!(
            "No timestamp field found in any of {} lines ({} valid JSON entries)",
            lines.len(),
            lines.len() - parse_errors
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_jsonl_with_timestamp() {
        let content = r#"{"timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":"test"}"#;
        let (is_valid, error) = validate_jsonl_timestamps(content);
        assert!(is_valid);
        assert!(error.is_none());
    }

    #[test]
    fn test_invalid_jsonl_no_timestamp() {
        let content = r#"{"type":"user","message":"test"}"#;
        let (is_valid, error) = validate_jsonl_timestamps(content);
        assert!(!is_valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_empty_content() {
        let (is_valid, error) = validate_jsonl_timestamps("");
        assert!(!is_valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_mixed_content() {
        let mixed_content = r#"{"type":"user","message":"test"}
{"timestamp":"2025-01-01T10:00:00.000Z","type":"assistant","message":"response"}"#;
        let (is_valid, error) = validate_jsonl_timestamps(mixed_content);
        assert!(is_valid);
        assert!(error.is_none());
    }
}
