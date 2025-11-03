// Agent file merging utilities for Claude Code sessions
//
// Claude Code creates separate agent files (agent-*.jsonl) for Task tool invocations.
// This module handles merging these agent files back into the main session file
// during cache generation, inserting agent messages after their corresponding tool_result.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Merge a main session file with its associated agent files
///
/// This function:
/// 1. Reads the main session JSONL line by line
/// 2. Detects tool_result messages with toolUseResult.agentId
/// 3. Loads the corresponding agent-*.jsonl file
/// 4. Inserts all agent messages after the tool_result message
/// 5. Writes the merged output to the cache path
///
/// Agent messages are preserved with their isSidechain:true flag,
/// allowing them to be filtered out later if needed.
///
/// # Arguments
/// * `source_file` - Path to the main session file
/// * `cache_path` - Destination path for the merged output
///
/// # Returns
/// Ok(()) if successful, Err with description if failed
pub fn merge_session_with_agents(
    source_file: &Path,
    cache_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Read main session content
    let content = fs::read_to_string(source_file)?;

    // Get the directory containing the source file (for finding agent files)
    let source_dir = source_file
        .parent()
        .ok_or("Source file has no parent directory")?;

    // Extract session ID from content for validation
    let session_id = extract_session_id_from_content(&content)?;

    // Create cache file and write merged content
    let mut output = Vec::new();

    for line in content.lines() {
        // Write the current line
        writeln!(output, "{}", line)?;

        // Check if this line contains toolUseResult.agentId
        if let Some(agent_id) = extract_agent_id_from_line(line) {
            // Load and append agent messages
            if let Ok(agent_messages) = load_agent_messages(source_dir, &agent_id, &session_id) {
                for agent_line in agent_messages {
                    writeln!(output, "{}", agent_line)?;
                }
            }
            // Silently ignore missing agent files (may not exist yet during partial writes)
        }
    }

    // Ensure cache directory exists
    if let Some(cache_dir) = cache_path.parent() {
        fs::create_dir_all(cache_dir)?;
    }

    // Write merged output to cache
    fs::write(cache_path, output)?;

    Ok(())
}

/// Extract agent ID from a JSONL line containing toolUseResult
///
/// Looks for JSON structure: { "toolUseResult": { "agentId": "abc123" } }
fn extract_agent_id_from_line(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;

    value
        .get("toolUseResult")?
        .get("agentId")?
        .as_str()
        .map(|s| s.to_string())
}

/// Extract session ID from the main session content
///
/// Reads the first few lines to find a message with sessionId field
fn extract_session_id_from_content(
    content: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    for line in content.lines().take(10) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(session_id) = value.get("sessionId").and_then(|v| v.as_str()) {
                return Ok(session_id.to_string());
            }
        }
    }
    Err("No sessionId found in session content".into())
}

/// Load agent messages from an agent file
///
/// Reads agent-{agent_id}.jsonl from the source directory and validates
/// that it belongs to the specified session.
///
/// # Arguments
/// * `source_dir` - Directory containing the agent file
/// * `agent_id` - The agent ID (e.g., "abc123")
/// * `expected_session_id` - The parent session ID for validation
///
/// # Returns
/// Vec of JSONL lines (strings) from the agent file, or error if not found/invalid
fn load_agent_messages(
    source_dir: &Path,
    agent_id: &str,
    expected_session_id: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let agent_file_path = source_dir.join(format!("agent-{}.jsonl", agent_id));

    if !agent_file_path.exists() {
        return Err(format!("Agent file not found: {}", agent_file_path.display()).into());
    }

    let content = fs::read_to_string(&agent_file_path)?;

    // Validate that this agent file belongs to the expected session
    // Check the first line for sessionId
    if let Some(first_line) = content.lines().next() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(first_line) {
            if let Some(agent_session_id) = value.get("sessionId").and_then(|v| v.as_str()) {
                if agent_session_id != expected_session_id {
                    return Err(format!(
                        "Agent file session ID mismatch: expected {}, got {}",
                        expected_session_id, agent_session_id
                    )
                    .into());
                }
            }
        }
    }

    // Return all lines from the agent file
    Ok(content.lines().map(|s| s.to_string()).collect())
}

/// Check if a filename is an agent file
///
/// Agent files match the pattern: agent-{8-hex-chars}.jsonl
pub fn is_agent_file(filename: &str) -> bool {
    filename.starts_with("agent-") && filename.ends_with(".jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_is_agent_file() {
        assert!(is_agent_file("agent-12345678.jsonl"));
        assert!(is_agent_file("agent-abc123de.jsonl"));
        assert!(!is_agent_file("session-123.jsonl"));
        assert!(!is_agent_file("abc-123.jsonl"));
        assert!(!is_agent_file("agent-123.txt"));
    }

    #[test]
    fn test_extract_agent_id_from_line() {
        let line = r#"{"type":"user","toolUseResult":{"agentId":"abc123","status":"completed"},"timestamp":"2025-01-01T10:00:00.000Z"}"#;
        assert_eq!(extract_agent_id_from_line(line), Some("abc123".to_string()));

        let line_no_agent = r#"{"type":"user","timestamp":"2025-01-01T10:00:00.000Z"}"#;
        assert_eq!(extract_agent_id_from_line(line_no_agent), None);
    }

    #[test]
    fn test_extract_session_id_from_content() {
        let content = r#"{"sessionId":"session-123","timestamp":"2025-01-01T10:00:00.000Z"}
{"sessionId":"session-123","type":"user","message":{"role":"user","content":"Hello"}}"#;

        let session_id = extract_session_id_from_content(content).unwrap();
        assert_eq!(session_id, "session-123");
    }

    #[test]
    fn test_merge_session_with_agents() {
        let temp_dir = tempdir().unwrap();

        // Create main session file
        let main_session = temp_dir.path().join("session-123.jsonl");
        let main_content = r#"{"sessionId":"session-123","timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":{"role":"user","content":"Hello"}}
{"sessionId":"session-123","timestamp":"2025-01-01T10:01:00.000Z","type":"user","toolUseResult":{"agentId":"abc123","status":"completed"}}
{"sessionId":"session-123","timestamp":"2025-01-01T10:02:00.000Z","type":"assistant","message":{"role":"assistant","content":"Done"}}"#;
        fs::write(&main_session, main_content).unwrap();

        // Create agent file
        let agent_file = temp_dir.path().join("agent-abc123.jsonl");
        let agent_content = r#"{"sessionId":"session-123","agentId":"abc123","isSidechain":true,"timestamp":"2025-01-01T10:01:10.000Z","type":"user","message":{"role":"user","content":"Agent task"}}
{"sessionId":"session-123","agentId":"abc123","isSidechain":true,"timestamp":"2025-01-01T10:01:20.000Z","type":"assistant","message":{"role":"assistant","content":"Agent response"}}"#;
        fs::write(&agent_file, agent_content).unwrap();

        // Merge
        let cache_path = temp_dir.path().join("cache").join("merged.jsonl");
        merge_session_with_agents(&main_session, &cache_path).unwrap();

        // Verify merged content
        let merged = fs::read_to_string(&cache_path).unwrap();
        let lines: Vec<&str> = merged.lines().collect();

        assert_eq!(lines.len(), 5); // 3 main + 2 agent
        assert!(lines[0].contains("Hello"));
        assert!(lines[1].contains("toolUseResult"));
        assert!(lines[2].contains("Agent task")); // First agent message
        assert!(lines[3].contains("Agent response")); // Second agent message
        assert!(lines[4].contains("Done"));

        // Verify agent messages have isSidechain flag
        assert!(lines[2].contains("isSidechain"));
        assert!(lines[3].contains("isSidechain"));
    }

    #[test]
    fn test_merge_handles_missing_agent_file() {
        let temp_dir = tempdir().unwrap();

        // Create main session file with reference to non-existent agent
        let main_session = temp_dir.path().join("session-456.jsonl");
        let main_content = r#"{"sessionId":"session-456","timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":{"role":"user","content":"Hello"}}
{"sessionId":"session-456","timestamp":"2025-01-01T10:01:00.000Z","type":"user","toolUseResult":{"agentId":"missing","status":"completed"}}
{"sessionId":"session-456","timestamp":"2025-01-01T10:02:00.000Z","type":"assistant","message":{"role":"assistant","content":"Done"}}"#;
        fs::write(&main_session, main_content).unwrap();

        // Merge should succeed even without agent file
        let cache_path = temp_dir.path().join("cache").join("merged.jsonl");
        merge_session_with_agents(&main_session, &cache_path).unwrap();

        // Verify content (no agent messages inserted)
        let merged = fs::read_to_string(&cache_path).unwrap();
        let lines: Vec<&str> = merged.lines().collect();

        assert_eq!(lines.len(), 3); // Only main session lines
    }
}
