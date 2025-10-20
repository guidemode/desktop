use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    pub completed: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeSession {
    pub id: String,
    pub version: Option<String>,
    #[serde(rename = "projectID")]
    pub project_id: Option<String>,
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
    // Tool-specific fields
    pub tool: Option<String>,
    #[serde(rename = "callID")]
    pub call_id: Option<String>,
    pub state: Option<OpenCodeToolState>,
    // Step-finish fields
    pub tokens: Option<OpenCodeTokens>,
    pub cost: Option<f64>,
    // Patch fields
    pub files: Option<Vec<String>>,
    pub hash: Option<String>,
    // Snapshot fields
    pub snapshot: Option<String>,
    // File fields
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub url: Option<String>,
    pub source: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeToolState {
    pub status: String,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub title: Option<String>,
    pub time: Option<OpenCodePartTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeTokens {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning: Option<i64>,
    pub cache: Option<OpenCodeTokenCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeTokenCache {
    pub write: Option<i64>,
    pub read: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodePartTime {
    pub start: Option<i64>,
    pub end: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeJsonLEntry {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub message: OpenCodeJsonLMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeJsonLMessage {
    pub role: String,
    pub content: Vec<OpenCodeJsonLContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenCodeJsonLContent {
    Text {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
    },
    ToolUse {
        #[serde(rename = "type")]
        content_type: String,
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        #[serde(rename = "type")]
        content_type: String,
        tool_use_id: String,
        content: String,
        is_error: Option<bool>,
    },
    File {
        #[serde(rename = "type")]
        content_type: String,
        filename: String,
        mime: String,
        url: String,
    },
    Patch {
        #[serde(rename = "type")]
        content_type: String,
        files: Vec<String>,
        hash: String,
    },
}

#[derive(Debug, Clone)]
pub struct ParsedSession {
    pub session_id: String,
    pub project_name: String,
    pub session_start_time: Option<DateTime<Utc>>,
    pub session_end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub jsonl_content: String,
    #[allow(dead_code)]
    pub total_tokens: Option<OpenCodeTokens>,
    #[allow(dead_code)]
    pub total_cost: Option<f64>,
    #[allow(dead_code)]
    pub tool_count: usize,
    #[allow(dead_code)]
    pub file_count: usize,
    pub cwd: Option<String>,
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
        let project_id = session
            .project_id
            .ok_or_else(|| format!("Session {} has no project ID", session_id))?;
        let project = self.load_project(&project_id)?;
        let cwd = Some(project.worktree.clone());

        // Load all messages for this session
        let messages = self.load_messages_for_session(session_id)?;

        // Track aggregated metrics
        let mut total_input_tokens = 0i64;
        let mut total_output_tokens = 0i64;
        let mut total_reasoning_tokens = 0i64;
        let mut total_cache_write = 0i64;
        let mut total_cache_read = 0i64;
        let mut total_cost = 0.0;
        let mut tool_count = 0usize;
        let mut file_count = 0usize;

        // Load all parts for each message and organize chronologically
        let mut session_entries = Vec::new();

        for message in messages {
            let parts = self.load_parts_for_message(&message.id)?;

            // Process parts and create separate entries for tool use/results
            let mut text_content: Vec<OpenCodeJsonLContent> = Vec::new();

            let base_timestamp = message
                .time
                .created
                .or(message.time.completed) // Messages use 'completed' not 'initialized/updated'
                .or(message.time.initialized)
                .or(message.time.updated)
                .and_then(DateTime::from_timestamp_millis)
                .unwrap_or_else(Utc::now);

            for part in parts {
                match part.part_type.as_str() {
                    "text" => {
                        if let Some(text) = part.text {
                            text_content.push(OpenCodeJsonLContent::Text {
                                content_type: "text".to_string(),
                                text,
                            });
                        }
                    }
                    "tool" => {
                        if let (Some(tool_name), Some(call_id), Some(state)) = (
                            part.tool.as_ref(),
                            part.call_id.as_ref(),
                            part.state.as_ref(),
                        ) {
                            tool_count += 1;

                            // Get timestamp from part if available
                            let part_timestamp = part
                                .time
                                .as_ref()
                                .and_then(|t| t.start)
                                .and_then(DateTime::from_timestamp_millis)
                                .unwrap_or(base_timestamp);

                            // Create separate entry for tool use
                            let tool_use_entry = OpenCodeJsonLEntry {
                                session_id: session_id.to_string(),
                                timestamp: part_timestamp.to_rfc3339(),
                                entry_type: "tool_use".to_string(),
                                message: OpenCodeJsonLMessage {
                                    role: "tool".to_string(),
                                    content: vec![OpenCodeJsonLContent::ToolUse {
                                        content_type: "tool_use".to_string(),
                                        id: call_id.clone(),
                                        name: tool_name.clone(),
                                        input: state
                                            .input
                                            .clone()
                                            .unwrap_or(serde_json::Value::Null),
                                    }],
                                },
                                cwd: cwd.clone(),
                            };
                            session_entries.push((part_timestamp, tool_use_entry));

                            // Create separate entry for tool result if output exists
                            if let Some(output) = state.output.as_ref() {
                                let result_timestamp = part
                                    .time
                                    .as_ref()
                                    .and_then(|t| t.end)
                                    .and_then(DateTime::from_timestamp_millis)
                                    .unwrap_or_else(|| {
                                        part_timestamp + chrono::Duration::milliseconds(1)
                                    });

                                let tool_result_entry = OpenCodeJsonLEntry {
                                    session_id: session_id.to_string(),
                                    timestamp: result_timestamp.to_rfc3339(),
                                    entry_type: "tool_result".to_string(),
                                    message: OpenCodeJsonLMessage {
                                        role: "tool".to_string(),
                                        content: vec![OpenCodeJsonLContent::ToolResult {
                                            content_type: "tool_result".to_string(),
                                            tool_use_id: call_id.clone(),
                                            content: output.clone(),
                                            is_error: Some(state.status != "completed"),
                                        }],
                                    },
                                    cwd: cwd.clone(),
                                };
                                session_entries.push((result_timestamp, tool_result_entry));
                            }
                        }
                    }
                    "file" => {
                        if let (Some(filename), Some(mime), Some(url)) = (
                            part.filename.as_ref(),
                            part.mime.as_ref(),
                            part.url.as_ref(),
                        ) {
                            file_count += 1;
                            text_content.push(OpenCodeJsonLContent::File {
                                content_type: "file".to_string(),
                                filename: filename.clone(),
                                mime: mime.clone(),
                                url: url.clone(),
                            });
                        }
                    }
                    "patch" => {
                        if let (Some(files), Some(hash)) = (part.files.as_ref(), part.hash.as_ref())
                        {
                            if !files.is_empty() {
                                text_content.push(OpenCodeJsonLContent::Patch {
                                    content_type: "patch".to_string(),
                                    files: files.clone(),
                                    hash: hash.clone(),
                                });
                            }
                        }
                    }
                    "step-finish" => {
                        // Aggregate token usage
                        if let Some(tokens) = part.tokens.as_ref() {
                            total_input_tokens += tokens.input.unwrap_or(0);
                            total_output_tokens += tokens.output.unwrap_or(0);
                            total_reasoning_tokens += tokens.reasoning.unwrap_or(0);
                            if let Some(cache) = tokens.cache.as_ref() {
                                total_cache_write += cache.write.unwrap_or(0);
                                total_cache_read += cache.read.unwrap_or(0);
                            }
                        }
                        if let Some(cost) = part.cost {
                            total_cost += cost;
                        }
                    }
                    _ => {
                        // Skip other types (step-start, snapshot, etc.)
                    }
                }
            }

            // Create entry for text/file/patch content if any
            if !text_content.is_empty() {
                let entry = OpenCodeJsonLEntry {
                    session_id: session_id.to_string(),
                    timestamp: base_timestamp.to_rfc3339(),
                    entry_type: message.role.clone(),
                    message: OpenCodeJsonLMessage {
                        role: message.role,
                        content: text_content,
                    },
                    cwd: cwd.clone(),
                };

                session_entries.push((base_timestamp, entry));
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

        // Build aggregated token data
        let total_tokens = if total_input_tokens > 0 || total_output_tokens > 0 {
            Some(OpenCodeTokens {
                input: Some(total_input_tokens),
                output: Some(total_output_tokens),
                reasoning: if total_reasoning_tokens > 0 {
                    Some(total_reasoning_tokens)
                } else {
                    None
                },
                cache: if total_cache_read > 0 || total_cache_write > 0 {
                    Some(OpenCodeTokenCache {
                        write: if total_cache_write > 0 {
                            Some(total_cache_write)
                        } else {
                            None
                        },
                        read: if total_cache_read > 0 {
                            Some(total_cache_read)
                        } else {
                            None
                        },
                    })
                } else {
                    None
                },
            })
        } else {
            None
        };

        Ok(ParsedSession {
            session_id: session_id.to_string(),
            project_name,
            session_start_time,
            session_end_time,
            duration_ms,
            jsonl_content,
            total_tokens,
            total_cost: if total_cost > 0.0 {
                Some(total_cost)
            } else {
                None
            },
            tool_count,
            file_count,
            cwd: Some(project.worktree.clone()),
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

                    let mut session: OpenCodeSession = serde_json::from_str(&content)
                        .map_err(|e| format!("Failed to parse session JSON: {}", e))?;

                    // If projectID is not in the JSON, infer it from the directory path
                    if session.project_id.is_none() {
                        if let Some(project_id) = project_session_dir
                            .file_name()
                            .and_then(|name| name.to_str())
                        {
                            session.project_id = Some(project_id.to_string());
                        }
                    }

                    return Ok(session);
                }
            }
        }

        Err(format!("Session {} not found", session_id))
    }

    fn load_project(&self, project_id: &str) -> Result<OpenCodeProject, String> {
        let project_file = self
            .storage_path
            .join("project")
            .join(format!("{}.json", project_id));
        self.load_project_from_path(&project_file)
    }

    fn load_project_from_path(&self, path: &Path) -> Result<OpenCodeProject, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read project file: {}", e))?;

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
            msg.time
                .created
                .or(msg.time.completed)
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

        let entries =
            fs::read_dir(&part_dir).map_err(|e| format!("Failed to read part directory: {}", e))?;

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
        parts.sort_by_key(|part| part.time.as_ref().and_then(|t| t.start).unwrap_or(0));

        Ok(parts)
    }

    pub fn get_session_for_part(&self, part_path: &Path) -> Option<String> {
        // Extract message ID from part path
        // Path format: ~/.local/share/opencode/storage/part/{messageId}/{partId}.json
        let message_id = part_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())?;

        // Messages are stored in: storage/message/{sessionId}/{messageId}.json
        // We need to scan session directories to find which one contains this messageId
        let message_base_dir = self.storage_path.join("message");

        if !message_base_dir.exists() {
            return None;
        }

        // Iterate through session directories
        if let Ok(entries) = fs::read_dir(&message_base_dir) {
            for entry in entries.flatten() {
                let session_dir = entry.path();
                if session_dir.is_dir() {
                    let message_file = session_dir.join(format!("{}.json", message_id));

                    if message_file.exists() {
                        // Found the message file - read it to get the session ID
                        if let Ok(content) = fs::read_to_string(&message_file) {
                            if let Ok(message) = serde_json::from_str::<OpenCodeMessage>(&content) {
                                return Some(message.session_id);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    pub fn get_project_for_session(&self, session_id: &str) -> Option<String> {
        if let Ok(session) = self.load_session(session_id) {
            session.project_id
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
        fs::write(
            storage_path.join("project").join("test_project.json"),
            project,
        )
        .unwrap();

        // Create test session (without projectID - will be inferred from directory)
        let session =
            r#"{"id":"test_session","title":"Test Session","time":{"created":1609459200000}}"#;
        fs::write(
            storage_path
                .join("session")
                .join("test_project")
                .join("test_session.json"),
            session,
        )
        .unwrap();

        // Create test message
        let message = r#"{"id":"test_message","role":"user","sessionID":"test_session","time":{"created":1609459200000}}"#;
        fs::write(
            storage_path
                .join("message")
                .join("test_session")
                .join("test_message.json"),
            message,
        )
        .unwrap();

        // Create test part with text
        let part = r#"{"id":"test_part","type":"text","text":"Hello, world!","messageID":"test_message","sessionID":"test_session"}"#;
        fs::write(
            storage_path
                .join("part")
                .join("test_message")
                .join("test_part.json"),
            part,
        )
        .unwrap();

        // Create test part with tool use and result
        let tool_part = r#"{"id":"test_tool_part","type":"tool","tool":"read","callID":"call_123","state":{"status":"completed","input":{"filePath":"/test/file.txt"},"output":"File contents here","title":"test"},"messageID":"test_message","sessionID":"test_session"}"#;
        fs::write(
            storage_path
                .join("part")
                .join("test_message")
                .join("test_tool_part.json"),
            tool_part,
        )
        .unwrap();

        let parser = OpenCodeParser::new(storage_path);
        (temp_dir, parser)
    }

    #[test]
    fn test_parse_session() {
        let (_temp_dir, parser) = create_test_opencode_structure();

        let result = parser.parse_session("test_session").unwrap();

        assert_eq!(result.session_id, "test_session");
        assert_eq!(result.project_name, "project");
        assert!(!result.jsonl_content.is_empty());

        // Print JSONL for debugging
        println!("Generated JSONL:\n{}", result.jsonl_content);
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

    #[test]
    fn test_message_timestamp_with_completed_field() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");

        fs::create_dir_all(storage_path.join("project")).unwrap();
        fs::create_dir_all(storage_path.join("session").join("test_project")).unwrap();
        fs::create_dir_all(storage_path.join("message").join("test_session")).unwrap();
        fs::create_dir_all(storage_path.join("part").join("test_message")).unwrap();

        // Create project
        let project = r#"{"id":"test_project","worktree":"/path/to/project","time":{"created":1000000000000}}"#;
        fs::write(
            storage_path.join("project").join("test_project.json"),
            project,
        )
        .unwrap();

        // Create session
        let session = r#"{"id":"test_session","projectID":"test_project","time":{"created":1000000000000}}"#;
        fs::write(
            storage_path
                .join("session")
                .join("test_project")
                .join("test_session.json"),
            session,
        )
        .unwrap();

        // Create message with 'completed' timestamp (not 'updated')
        let message = r#"{"id":"test_message","role":"assistant","sessionID":"test_session","time":{"created":1000000000000,"completed":1000000001000}}"#;
        fs::write(
            storage_path
                .join("message")
                .join("test_session")
                .join("test_message.json"),
            message,
        )
        .unwrap();

        // Create part
        let part = r#"{"id":"test_part","type":"text","text":"Test message","messageID":"test_message","sessionID":"test_session"}"#;
        fs::write(
            storage_path
                .join("part")
                .join("test_message")
                .join("test_part.json"),
            part,
        )
        .unwrap();

        let parser = OpenCodeParser::new(storage_path);
        let result = parser.parse_session("test_session").unwrap();

        // Should have proper timestamps from 'completed' field
        assert!(result.session_start_time.is_some());
        assert!(result.jsonl_content.contains("2001-09-09")); // Timestamp should be from 'completed' field
    }

    #[test]
    fn test_get_session_for_part_correct_path() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("storage");

        fs::create_dir_all(storage_path.join("message").join("ses_123")).unwrap();
        fs::create_dir_all(storage_path.join("part").join("msg_456")).unwrap();

        // Create message file in correct location: message/{sessionId}/{messageId}.json
        let message = r#"{"id":"msg_456","role":"user","sessionID":"ses_123","time":{"created":1000000000000}}"#;
        fs::write(
            storage_path.join("message").join("ses_123").join("msg_456.json"),
            message,
        )
        .unwrap();

        let parser = OpenCodeParser::new(storage_path.clone());
        let part_path = storage_path.join("part").join("msg_456").join("part_789.json");

        let session_id = parser.get_session_for_part(&part_path);

        assert_eq!(session_id, Some("ses_123".to_string()));
    }

    #[test]
    fn test_jsonl_includes_cwd_field() {
        let (_temp_dir, parser) = create_test_opencode_structure();

        let result = parser.parse_session("test_session").unwrap();

        // Check that JSONL contains cwd field
        assert!(result.jsonl_content.contains("\"cwd\""));
        assert!(result.jsonl_content.contains("/path/to/project"));
    }
}
