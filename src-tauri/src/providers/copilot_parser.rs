use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::fs;
use std::path::{Path, PathBuf};

/// GitHub Copilot config.json format
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CopilotConfig {
    #[serde(default)]
    pub trusted_folders: Vec<String>,
}

/// GitHub Copilot event-based session format (new)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: serde_json::Value,
    pub id: String,
    pub timestamp: String,
    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,
}

/// Session start event data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartData {
    pub session_id: String,
    pub version: u32,
    pub producer: String,
    pub copilot_version: String,
    pub start_time: String,
}

/// Tool execution arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ToolArguments {
    pub command: Option<String>,
    pub path: Option<String>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParsedSession {
    pub session_id: String,
    pub project_name: String,
    pub session_start_time: Option<DateTime<Utc>>,
    pub session_end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub jsonl_content: String,
    pub cwd: Option<String>,
}

pub struct CopilotParser {
    #[allow(dead_code)]
    storage_path: PathBuf,
}

/// Load the Copilot config.json file to get trusted folders
pub fn load_copilot_config() -> Result<CopilotConfig, String> {
    let config_path = tilde("~/.copilot/config.json");
    let config_path = Path::new(config_path.as_ref());

    if !config_path.exists() {
        return Ok(CopilotConfig {
            trusted_folders: Vec::new(),
        });
    }

    let content = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read copilot config: {}", e))?;

    let config: CopilotConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse copilot config: {}", e))?;

    Ok(config)
}

/// Detect project name AND cwd path from events by matching against trusted folders
/// Returns (project_name, cwd_path)
/// Scans ALL events and returns on first match with a trusted folder
pub fn detect_project_and_cwd_from_events(
    events: &[CopilotEvent],
    trusted_folders: &[String],
) -> Option<(String, String)> {
    // Scan events to find tool.execution_start events with path in arguments
    for event in events {
        if event.event_type == "tool.execution_start" {
            if let Some(args) = event.data.get("arguments") {
                // Try to extract path from arguments
                if let Some(path) = args.get("path").and_then(|p| p.as_str()) {
                    // Match against trusted folders
                    if let Some((project, cwd)) = match_trusted_folder_with_cwd(path, trusted_folders) {
                        return Some((project, cwd));
                    }
                }

                // Also check if command contains a path (for bash commands like cd /path)
                if let Some(command) = args.get("command").and_then(|c| c.as_str()) {
                    // Try to extract paths from common commands
                    for word in command.split_whitespace() {
                        if word.starts_with('/') || word.starts_with('~') {
                            if let Some((project, cwd)) = match_trusted_folder_with_cwd(word, trusted_folders) {
                                return Some((project, cwd));
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Match a path against trusted folders, returning (folder_name, folder_path) if matched
fn match_trusted_folder_with_cwd(
    path: &str,
    trusted_folders: &[String],
) -> Option<(String, String)> {
    for folder in trusted_folders {
        let expanded_folder = tilde(folder);
        let folder_path = expanded_folder.as_ref();

        // Check if the path starts with this trusted folder
        if path.starts_with(folder_path) {
            // Extract the folder name (last component of the path)
            if let Some(name) = Path::new(folder_path).file_name().and_then(|n| n.to_str()) {
                return Some((name.to_string(), folder_path.to_string()));
            }
        }
    }

    None
}

/// Match a path against trusted folders, returning the folder name if matched
#[allow(dead_code)]
fn match_trusted_folder(path: &str, trusted_folders: &[String]) -> Option<String> {
    match_trusted_folder_with_cwd(path, trusted_folders).map(|(name, _cwd)| name)
}

impl CopilotParser {
    #[allow(dead_code)]
    pub fn new(storage_path: PathBuf) -> Self {
        Self { storage_path }
    }

    #[allow(dead_code)]
    pub fn parse_session(&self, session_file_path: &Path) -> Result<ParsedSession, String> {
        // Read the JSONL file
        let content = fs::read_to_string(session_file_path)
            .map_err(|e| format!("Failed to read session file: {}", e))?;

        // Parse JSONL - one event per line
        let mut events = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let event: CopilotEvent = serde_json::from_str(line)
                .map_err(|e| format!("Failed to parse event line: {}", e))?;
            events.push(event);
        }

        if events.is_empty() {
            return Err("No events found in session file".to_string());
        }

        // Extract session metadata from session.start event
        let session_start_event = events
            .iter()
            .find(|e| e.event_type == "session.start")
            .ok_or("No session.start event found")?;

        let session_start_data: SessionStartData = serde_json::from_value(session_start_event.data.clone())
            .map_err(|e| format!("Failed to parse session.start data: {}", e))?;

        // Get start and end times
        let session_start_time = DateTime::parse_from_rfc3339(&session_start_data.start_time)
            .ok()
            .map(|dt| dt.with_timezone(&Utc));

        // Last event timestamp is the end time
        let session_end_time = events.last().and_then(|e| {
            DateTime::parse_from_rfc3339(&e.timestamp)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        // Calculate duration
        let duration_ms = match (session_start_time, session_end_time) {
            (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
            _ => None,
        };

        // Detect project and cwd from events
        let (project_name, cwd) = match load_copilot_config() {
            Ok(config) if !config.trusted_folders.is_empty() => {
                detect_project_and_cwd_from_events(&events, &config.trusted_folders)
                    .map(|(name, cwd)| (name, Some(cwd)))
                    .unwrap_or_else(|| ("copilot-sessions".to_string(), None))
            }
            _ => ("copilot-sessions".to_string(), None),
        };

        // Extract session ID from filename (UUID)
        let session_id = session_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| session_start_data.session_id.clone());

        // Convert events to canonical format
        use crate::providers::copilot::converter::convert_event_to_canonical;

        let mut canonical_messages = Vec::new();
        for event in &events {
            match convert_event_to_canonical(event, &session_id, cwd.as_deref()) {
                Ok(mut messages) => canonical_messages.append(&mut messages),
                Err(e) => {
                    // Log error but continue processing other events
                    eprintln!("Warning: Failed to convert event: {}", e);
                }
            }
        }

        // Convert canonical messages to JSONL
        let jsonl_content = canonical_messages
            .iter()
            .filter_map(|msg| serde_json::to_string(msg).ok())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ParsedSession {
            session_id,
            project_name,
            session_start_time,
            session_end_time,
            duration_ms,
            jsonl_content,
            cwd,
        })
    }

    #[allow(dead_code)]
    pub fn get_all_sessions(&self) -> Result<Vec<PathBuf>, String> {
        let session_dir = self.storage_path.join("session-state");
        if !session_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&session_dir)
            .map_err(|e| format!("Failed to read session directory: {}", e))?;

        let mut session_files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                // Skip hidden files
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !file_name.starts_with('.') {
                        session_files.push(path);
                    }
                }
            }
        }

        // Sort by modification time (most recent first)
        session_files.sort_by(|a, b| {
            let a_modified = fs::metadata(a).and_then(|m| m.modified()).ok();
            let b_modified = fs::metadata(b).and_then(|m| m.modified()).ok();
            b_modified.cmp(&a_modified)
        });

        Ok(session_files)
    }

    #[allow(dead_code)]
    pub fn get_sessions_for_project(&self, _project_name: &str) -> Result<Vec<PathBuf>, String> {
        // Copilot doesn't have separate projects, so just return all sessions
        self.get_all_sessions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_copilot_session() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path();
        let session_dir = storage_path.join("session-state");
        fs::create_dir_all(&session_dir).unwrap();

        // Create a test session file in new JSONL event format
        let session_content = r#"{"type":"session.start","data":{"sessionId":"test-session-123","version":1,"producer":"copilot-agent","copilotVersion":"0.0.348","startTime":"2025-01-01T10:00:00.000Z"},"id":"event-1","timestamp":"2025-01-01T10:00:00.000Z","parentId":null}
{"type":"user.message","data":{"content":"Hello","attachments":[]},"id":"event-2","timestamp":"2025-01-01T10:00:05.000Z","parentId":"event-1"}
{"type":"assistant.message","data":{"messageId":"msg-1","content":"I can help you with coding tasks.","toolRequests":[]},"id":"event-3","timestamp":"2025-01-01T10:00:10.000Z","parentId":"event-2"}"#;

        let session_file = session_dir.join("test-session-123.jsonl");
        fs::write(&session_file, session_content).unwrap();

        // Parse the session
        let parser = CopilotParser::new(storage_path.to_path_buf());
        let result = parser.parse_session(&session_file).unwrap();

        assert_eq!(result.session_id, "test-session-123");
        assert_eq!(result.project_name, "copilot-sessions");
        assert!(!result.jsonl_content.is_empty());
        assert!(result.session_start_time.is_some());
        assert!(result.session_end_time.is_some());

        // Verify it's canonical format (should have multiple lines, each a CanonicalMessage)
        let lines: Vec<&str> = result.jsonl_content.lines().collect();
        assert!(lines.len() >= 3, "Expected at least 3 canonical messages");

        // Parse first line to verify it's canonical format (uses camelCase)
        let first_msg: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert!(first_msg.get("uuid").is_some(), "Should have uuid field");
        assert!(first_msg.get("timestamp").is_some(), "Should have timestamp field");
        assert!(first_msg.get("type").is_some(), "Should have type field"); // message_type -> type
        assert!(first_msg.get("sessionId").is_some(), "Should have sessionId field"); // camelCase
        assert_eq!(first_msg.get("provider").and_then(|v| v.as_str()), Some("github-copilot"));
    }

    #[test]
    fn test_get_all_sessions() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path();
        let session_dir = storage_path.join("session-state");
        fs::create_dir_all(&session_dir).unwrap();

        // Create multiple session files (UUID-based naming)
        for i in 1..=3 {
            let session_file = session_dir.join(format!("test-session-{}.jsonl", i));
            fs::write(&session_file, "{}").unwrap();
        }

        // Create a non-JSONL file (should be ignored)
        fs::write(session_dir.join("config.json"), "{}").unwrap();

        let parser = CopilotParser::new(storage_path.to_path_buf());
        let sessions = parser.get_all_sessions().unwrap();

        assert_eq!(sessions.len(), 3);
    }
}
