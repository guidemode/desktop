//! Shared utilities for converting Claude Code sessions to canonical format

use crate::logging::log_debug;
use crate::providers::canonical::converter::ToCanonical;
use crate::providers::claude::types::ClaudeEntry;
use crate::providers::common::get_canonical_path;
use std::fs;
use std::path::{Path, PathBuf};

/// Convert a Claude Code session file to canonical format
///
/// This function:
/// 1. Reads the native Claude JSONL
/// 2. Parses each line as ClaudeEntry
/// 3. Filters out system events (file-history-snapshot, summary, etc.)
/// 4. Adds `provider: "claude-code"` field
/// 5. Fixes empty tool_result content
/// 6. Merges agent sidechain files
/// 7. Writes canonical JSONL to cache
///
/// # Arguments
/// * `claude_file` - Path to native Claude session file
/// * `session_id` - Session ID (from filename or content)
/// * `cwd` - Optional working directory (will be extracted if not provided)
///
/// # Returns
/// Path to the canonical JSONL file
pub fn convert_to_canonical_file(
    claude_file: &Path,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    // Read native Claude Code JSONL
    let content = fs::read_to_string(claude_file)?;

    // Get source directory for finding agent files
    let source_dir = claude_file
        .parent()
        .ok_or("Source file has no parent directory")?;

    let mut canonical_lines = Vec::new();
    let mut cwd_value: Option<String> = cwd.map(|s| s.to_string());

    // Parse and convert each line independently
    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<ClaudeEntry>(line) {
            Ok(claude_entry) => {
                // Extract CWD from first entry that has it (if not provided)
                if cwd_value.is_none() {
                    cwd_value = claude_entry.extract_cwd();
                }

                // Convert to canonical format (filters out system events)
                match claude_entry.to_canonical() {
                    Ok(Some(mut canonical_msg)) => {
                        // Ensure session_id is set correctly
                        canonical_msg.session_id = session_id.to_string();

                        canonical_lines.push(serde_json::to_string(&canonical_msg)?);

                        // Check if this message has an agent sidechain
                        if let Some(agent_id) = extract_agent_id_from_tool_use_result(&claude_entry)
                        {
                            // Load and convert agent messages
                            if let Ok(agent_lines) =
                                load_and_convert_agent_messages(source_dir, &agent_id, session_id)
                            {
                                canonical_lines.extend(agent_lines);
                            }
                        }
                    }
                    Ok(None) => {
                        // Message was filtered out (e.g., file-history-snapshot)
                    }
                    Err(e) => {
                        // Log parsing errors but continue processing
                        if let Err(log_err) = log_debug(
                            "claude-code",
                            &format!("Failed to convert line {}: {}", line_num + 1, e),
                        ) {
                            eprintln!("Logging error: {}", log_err);
                        }
                    }
                }
            }
            Err(e) => {
                // Log parsing errors but continue processing
                if let Err(log_err) = log_debug(
                    "claude-code",
                    &format!("Failed to parse line {}: {}", line_num + 1, e),
                ) {
                    eprintln!("Logging error: {}", log_err);
                }
            }
        }
    }

    // Get project-organized canonical path
    // Uses ~/.guideai/sessions/{provider}/{project}/{session_id}.jsonl
    let canonical_path = get_canonical_path("claude-code", cwd_value.as_deref(), session_id)?;

    // Write converted canonical JSONL
    if let Some(parent) = canonical_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&canonical_path, canonical_lines.join("\n"))?;

    Ok(canonical_path)
}

/// Extract agent ID from a Claude entry's toolUseResult
fn extract_agent_id_from_tool_use_result(entry: &ClaudeEntry) -> Option<String> {
    entry
        .tool_use_result
        .as_ref()?
        .get("agentId")?
        .as_str()
        .map(|s| s.to_string())
}

/// Load and convert agent messages from agent-*.jsonl file
fn load_and_convert_agent_messages(
    source_dir: &Path,
    agent_id: &str,
    session_id: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let agent_file = source_dir.join(format!("agent-{}.jsonl", agent_id));

    if !agent_file.exists() {
        // Agent file may not exist yet during partial writes
        return Ok(Vec::new());
    }

    let agent_content = fs::read_to_string(&agent_file)?;
    let mut agent_lines = Vec::new();

    for line in agent_content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(agent_entry) = serde_json::from_str::<ClaudeEntry>(line) {
            if let Ok(Some(mut canonical_msg)) = agent_entry.to_canonical() {
                // Ensure session_id is set correctly
                canonical_msg.session_id = session_id.to_string();

                if let Ok(json) = serde_json::to_string(&canonical_msg) {
                    agent_lines.push(json);
                }
            }
        }
    }

    Ok(agent_lines)
}
