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

/// GitHub Copilot session format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotSession {
    pub session_id: String,
    pub start_time: String,
    pub chat_messages: Vec<CopilotChatMessage>,
    #[serde(default)]
    pub timeline: Vec<TimelineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<CopilotToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotToolCall {
    pub function: CopilotFunction,
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotFunction {
    pub name: String,
    pub arguments: String,
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

/// Detect project name AND cwd path from timeline entries by matching against trusted folders
/// Returns (project_name, cwd_path)
/// Scans ALL timeline entries and returns on first match with a trusted folder
pub fn detect_project_and_cwd_from_timeline(
    timeline: &[TimelineEntry],
    trusted_folders: &[String],
) -> Option<(String, String)> {
    // Scan timeline entries to find one with an "arguments" property containing a "path"
    for entry in timeline {
        if let Some(args) = entry.data.get("arguments") {
            // Check if arguments is a string (might be JSON string)
            if let Some(args_str) = args.as_str() {
                // Try to parse as JSON
                if let Ok(args_json) = serde_json::from_str::<serde_json::Value>(args_str) {
                    if let Some(path) = args_json.get("path").and_then(|p| p.as_str()) {
                        // Match against trusted folders (partial match from the front)
                        if let Some((project, cwd)) =
                            match_trusted_folder_with_cwd(path, trusted_folders)
                        {
                            return Some((project, cwd));
                        }
                    }
                }
            }
            // Check if arguments is already an object
            else if let Some(path) = args.get("path").and_then(|p| p.as_str()) {
                // Match against trusted folders (partial match from the front)
                if let Some((project, cwd)) = match_trusted_folder_with_cwd(path, trusted_folders) {
                    return Some((project, cwd));
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
        // Read the session file
        let content = fs::read_to_string(session_file_path)
            .map_err(|e| format!("Failed to read session file: {}", e))?;

        // Parse the Copilot session format
        let copilot_session: CopilotSession = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse Copilot session JSON: {}", e))?;

        // Extract start and end times from timeline
        let (session_start_time, session_end_time) = if !copilot_session.timeline.is_empty() {
            // Find first entry with a timestamp
            let start = copilot_session.timeline.iter().find_map(|entry| {
                entry.timestamp.as_ref().and_then(|ts| {
                    DateTime::parse_from_rfc3339(ts)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                })
            });

            // Find last entry with a timestamp
            let end = copilot_session.timeline.iter().rev().find_map(|entry| {
                entry.timestamp.as_ref().and_then(|ts| {
                    DateTime::parse_from_rfc3339(ts)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                })
            });

            (start, end)
        } else {
            // Fallback to start_time from session if timeline is empty
            let start = DateTime::parse_from_rfc3339(&copilot_session.start_time)
                .ok()
                .map(|dt| dt.with_timezone(&Utc));
            (start, None)
        };

        // Convert timeline to JSONL format - minimal conversion
        // Each timeline entry becomes one JSONL line with no interpretation
        let jsonl_content = if !copilot_session.timeline.is_empty() {
            copilot_session
                .timeline
                .iter()
                .filter_map(|entry| {
                    // Create a simple JSON object combining timestamp and all data fields
                    let mut json_obj = serde_json::Map::new();

                    // Add timestamp if present
                    if let Some(ref ts) = entry.timestamp {
                        json_obj.insert(
                            "timestamp".to_string(),
                            serde_json::Value::String(ts.clone()),
                        );
                    }

                    // Add all other fields from the data
                    if let serde_json::Value::Object(data_map) = &entry.data {
                        for (key, value) in data_map {
                            json_obj.insert(key.clone(), value.clone());
                        }
                    }

                    // Serialize to JSON string (one line per entry)
                    serde_json::to_string(&json_obj).ok()
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            // No timeline available
            String::new()
        };

        // Calculate duration
        let duration_ms = match (session_start_time, session_end_time) {
            (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
            _ => None,
        };

        // Extract session ID from filename or use from JSON
        let session_id = session_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| {
                // Remove "session_" prefix if present
                s.strip_prefix("session_").unwrap_or(s).to_string()
            })
            .unwrap_or_else(|| copilot_session.session_id.clone());

        Ok(ParsedSession {
            session_id,
            project_name: "copilot-sessions".to_string(),
            session_start_time,
            session_end_time,
            duration_ms,
            jsonl_content,
            cwd: None,
        })
    }

    #[allow(dead_code)]
    pub fn get_all_sessions(&self) -> Result<Vec<PathBuf>, String> {
        let session_dir = self.storage_path.join("history-session-state");
        if !session_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&session_dir)
            .map_err(|e| format!("Failed to read session directory: {}", e))?;

        let mut session_files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                // Check if it's a session file (starts with "session_")
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.starts_with("session_") {
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
        let session_dir = storage_path.join("history-session-state");
        fs::create_dir_all(&session_dir).unwrap();

        // Create a test session file
        let session_content = r#"{
            "sessionId": "test-session-123",
            "startTime": "2025-01-01T10:00:00.000Z",
            "chatMessages": [
                {
                    "role": "user",
                    "content": "Hello, how can I help?"
                },
                {
                    "role": "assistant",
                    "content": "I can help you with coding tasks.",
                    "tool_calls": []
                }
            ],
            "timeline": [
                {
                    "timestamp": "2025-01-01T10:00:00.000Z",
                    "type": "user_message",
                    "content": "Hello"
                },
                {
                    "timestamp": "2025-01-01T10:00:05.000Z",
                    "type": "assistant_message",
                    "content": "I can help you with coding tasks."
                }
            ]
        }"#;

        let session_file = session_dir.join("session_test-session-123_1234567890.json");
        fs::write(&session_file, session_content).unwrap();

        // Parse the session
        let parser = CopilotParser::new(storage_path.to_path_buf());
        let result = parser.parse_session(&session_file).unwrap();

        assert_eq!(result.project_name, "copilot-sessions");
        assert!(!result.jsonl_content.is_empty());
        assert!(result.session_start_time.is_some());
    }

    #[test]
    fn test_get_all_sessions() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path();
        let session_dir = storage_path.join("history-session-state");
        fs::create_dir_all(&session_dir).unwrap();

        // Create multiple session files
        for i in 1..=3 {
            let session_file = session_dir.join(format!("session_test-{}_1234567890.json", i));
            fs::write(&session_file, "{}").unwrap();
        }

        // Create a non-session file (should be ignored)
        fs::write(session_dir.join("config.json"), "{}").unwrap();

        let parser = CopilotParser::new(storage_path.to_path_buf());
        let sessions = parser.get_all_sessions().unwrap();

        assert_eq!(sessions.len(), 3);
    }
}
