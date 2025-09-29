use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeProject {
    pub id: String,
    pub worktree: String,
    pub vcs: Option<String>,
    pub time: OpenCodeTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenCodeTime {
    pub created: Option<i64>,
    pub initialized: Option<i64>,
    pub updated: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeSession {
    pub id: String,
    pub version: Option<String>,
    #[serde(rename = "projectID")]
    pub project_id: String,
    pub directory: Option<String>,
    pub title: Option<String>,
    pub time: OpenCodeTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeMessage {
    pub id: String,
    pub role: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub time: OpenCodeTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodePart {
    pub id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: Option<String>,
    pub synthetic: Option<bool>,
    pub time: Option<OpenCodePartTime>,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodePartTime {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeJsonLEntry {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub message: OpenCodeJsonLMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeJsonLMessage {
    pub role: String,
    pub content: Vec<OpenCodeJsonLContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeJsonLContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ParsedSession {
    pub session_id: String,
    pub project_id: String,
    pub project_name: String,
    pub session_start_time: Option<DateTime<Utc>>,
    pub session_end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub jsonl_content: String,
}

pub struct OpenCodeParser {
    storage_path: PathBuf,
}

impl OpenCodeParser {
    pub fn new(storage_path: PathBuf) -> Self {
        Self { storage_path }
    }

    pub fn parse_session(&self, session_id: &str) -> Result<ParsedSession, String> {
        // Load session metadata first to get project ID
        let session = self.load_session(session_id)?;
        let project = self.load_project(&session.project_id)?;

        // Load all messages for this session
        let messages = self.load_messages_for_session(session_id)?;

        // Load all parts for each message and organize chronologically
        let mut session_entries = Vec::new();

        for message in messages {
            let parts = self.load_parts_for_message(&message.id)?;

            // Create JSONL entry for this message with all its parts
            let content: Vec<OpenCodeJsonLContent> = parts
                .into_iter()
                .filter_map(|part| {
                    part.text.map(|text| OpenCodeJsonLContent {
                        content_type: "text".to_string(),
                        text,
                    })
                })
                .collect();

            if !content.is_empty() {
                let timestamp = message.time.created
                    .or(message.time.initialized)
                    .or(message.time.updated)
                    .and_then(|ts| DateTime::from_timestamp_millis(ts))
                    .unwrap_or_else(|| Utc::now());

                let entry = OpenCodeJsonLEntry {
                    session_id: session_id.to_string(),
                    timestamp: timestamp.to_rfc3339(),
                    entry_type: message.role.clone(),
                    message: OpenCodeJsonLMessage {
                        role: message.role,
                        content,
                    },
                };

                session_entries.push((timestamp, entry));
            }
        }

        // Sort entries by timestamp
        session_entries.sort_by_key(|(timestamp, _)| *timestamp);

        // Build JSONL content
        let jsonl_content = session_entries
            .iter()
            .map(|(_, entry)| serde_json::to_string(entry).unwrap_or_default())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        // Calculate session timing
        let session_start_time = session_entries.first().map(|(ts, _)| *ts);
        let session_end_time = session_entries.last().map(|(ts, _)| *ts);
        let duration_ms = match (session_start_time, session_end_time) {
            (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
            _ => None,
        };

        // Extract project name from worktree path
        let project_name = Path::new(&project.worktree)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ParsedSession {
            session_id: session_id.to_string(),
            project_id: session.project_id,
            project_name,
            session_start_time,
            session_end_time,
            duration_ms,
            jsonl_content,
        })
    }

    pub fn get_sessions_for_project(&self, project_id: &str) -> Result<Vec<String>, String> {
        let session_dir = self.storage_path.join("session").join(project_id);
        if !session_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&session_dir)
            .map_err(|e| format!("Failed to read session directory: {}", e))?;

        let mut session_ids = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Some(session_id) = path.file_stem().and_then(|stem| stem.to_str()) {
                    session_ids.push(session_id.to_string());
                }
            }
        }

        Ok(session_ids)
    }

    pub fn get_all_projects(&self) -> Result<Vec<OpenCodeProject>, String> {
        let project_dir = self.storage_path.join("project");
        if !project_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&project_dir)
            .map_err(|e| format!("Failed to read project directory: {}", e))?;

        let mut projects = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                match self.load_project_from_path(&path) {
                    Ok(project) => projects.push(project),
                    Err(e) => {
                        eprintln!("Failed to load project from {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(projects)
    }

    fn load_session(&self, session_id: &str) -> Result<OpenCodeSession, String> {
        // We need to find the session file - it could be in any project directory
        let session_base_dir = self.storage_path.join("session");

        let entries = fs::read_dir(&session_base_dir)
            .map_err(|e| format!("Failed to read session base directory: {}", e))?;

        for entry in entries.flatten() {
            let project_session_dir = entry.path();
            if project_session_dir.is_dir() {
                let session_file = project_session_dir.join(format!("{}.json", session_id));
                if session_file.exists() {
                    let content = fs::read_to_string(&session_file)
                        .map_err(|e| format!("Failed to read session file: {}", e))?;

                    let session: OpenCodeSession = serde_json::from_str(&content)
                        .map_err(|e| format!("Failed to parse session JSON: {}", e))?;

                    return Ok(session);
                }
            }
        }

        Err(format!("Session {} not found", session_id))
    }

    fn load_project(&self, project_id: &str) -> Result<OpenCodeProject, String> {
        let project_file = self.storage_path.join("project").join(format!("{}.json", project_id));
        self.load_project_from_path(&project_file)
    }

    fn load_project_from_path(&self, path: &Path) -> Result<OpenCodeProject, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read project file: {}", e))?;

        let project: OpenCodeProject = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse project JSON: {}", e))?;

        Ok(project)
    }

    fn load_messages_for_session(&self, session_id: &str) -> Result<Vec<OpenCodeMessage>, String> {
        let message_dir = self.storage_path.join("message").join(session_id);
        if !message_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&message_dir)
            .map_err(|e| format!("Failed to read message directory: {}", e))?;

        let mut messages = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read message file: {}", e))?;

                let message: OpenCodeMessage = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse message JSON: {}", e))?;

                messages.push(message);
            }
        }

        // Sort messages by creation time
        messages.sort_by_key(|msg| {
            msg.time.created
                .or(msg.time.initialized)
                .or(msg.time.updated)
                .unwrap_or(0)
        });

        Ok(messages)
    }

    fn load_parts_for_message(&self, message_id: &str) -> Result<Vec<OpenCodePart>, String> {
        let part_dir = self.storage_path.join("part").join(message_id);
        if !part_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&part_dir)
            .map_err(|e| format!("Failed to read part directory: {}", e))?;

        let mut parts = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read part file: {}", e))?;

                let part: OpenCodePart = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse part JSON: {}", e))?;

                parts.push(part);
            }
        }

        // Sort parts by start time if available
        parts.sort_by_key(|part| {
            part.time.as_ref().map(|t| t.start).unwrap_or(0)
        });

        Ok(parts)
    }

    pub fn get_session_for_part(&self, part_path: &Path) -> Option<String> {
        // Extract session ID from part path
        // Path format: ~/.local/share/opencode/storage/part/{messageId}/{partId}.json
        if let Some(message_id) = part_path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
            // Load the message to get session ID
            if let Ok(content) = fs::read_to_string(self.storage_path.join("message").join(message_id).join(format!("{}.json", message_id))) {
                if let Ok(message) = serde_json::from_str::<OpenCodeMessage>(&content) {
                    return Some(message.session_id);
                }
            }
        }
        None
    }

    pub fn get_project_for_session(&self, session_id: &str) -> Option<String> {
        if let Ok(session) = self.load_session(session_id) {
            Some(session.project_id)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn create_test_opencode_structure() -> (tempfile::TempDir, OpenCodeParser) {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");

        // Create directory structure
        fs::create_dir_all(storage_path.join("project")).unwrap();
        fs::create_dir_all(storage_path.join("session").join("test_project")).unwrap();
        fs::create_dir_all(storage_path.join("message").join("test_session")).unwrap();
        fs::create_dir_all(storage_path.join("part").join("test_message")).unwrap();

        // Create test project
        let project = r#"{"id":"test_project","worktree":"/path/to/project","time":{"created":1609459200000}}"#;
        fs::write(storage_path.join("project").join("test_project.json"), project).unwrap();

        // Create test session
        let session = r#"{"id":"test_session","projectID":"test_project","title":"Test Session","time":{"created":1609459200000}}"#;
        fs::write(storage_path.join("session").join("test_project").join("test_session.json"), session).unwrap();

        // Create test message
        let message = r#"{"id":"test_message","role":"user","sessionID":"test_session","time":{"created":1609459200000}}"#;
        fs::write(storage_path.join("message").join("test_session").join("test_message.json"), message).unwrap();

        // Create test part
        let part = r#"{"id":"test_part","type":"text","text":"Hello, world!","messageID":"test_message","sessionID":"test_session"}"#;
        fs::write(storage_path.join("part").join("test_message").join("test_part.json"), part).unwrap();

        let parser = OpenCodeParser::new(storage_path);
        (temp_dir, parser)
    }

    #[test]
    fn test_parse_session() {
        let (_temp_dir, parser) = create_test_opencode_structure();

        let result = parser.parse_session("test_session").unwrap();

        assert_eq!(result.session_id, "test_session");
        assert_eq!(result.project_id, "test_project");
        assert_eq!(result.project_name, "project");
        assert!(!result.jsonl_content.is_empty());
    }

    #[test]
    fn test_get_sessions_for_project() {
        let (_temp_dir, parser) = create_test_opencode_structure();

        let sessions = parser.get_sessions_for_project("test_project").unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0], "test_session");
    }

    #[test]
    fn test_get_all_projects() {
        let (_temp_dir, parser) = create_test_opencode_structure();

        let projects = parser.get_all_projects().unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "test_project");
    }
}