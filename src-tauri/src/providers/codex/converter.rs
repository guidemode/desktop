use crate::providers::canonical::{
    converter::ToCanonical, CanonicalMessage, ContentBlock, ContentValue, MessageContent,
    MessageType, TokenUsage,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Codex JSONL message format
///
/// Codex messages have a consistent wrapper with timestamp, type, and payload.
/// The payload structure varies based on type.
#[derive(Debug, Clone, Serialize)]
pub struct CodexMessage {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub payload: CodexPayload,
}

// Custom deserializer for CodexMessage that uses message_type to determine payload variant
impl<'de> Deserialize<'de> for CodexMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;
        let timestamp = value.get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::missing_field("timestamp"))?
            .to_string();
        let message_type = value.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::missing_field("type"))?
            .to_string();

        let payload_value = value.get("payload")
            .ok_or_else(|| D::Error::missing_field("payload"))?;

        // Deserialize payload based on message_type
        let payload = match message_type.as_str() {
            "session_meta" => {
                CodexPayload::SessionMeta(serde_json::from_value(payload_value.clone())
                    .map_err(D::Error::custom)?)
            }
            "response_item" => {
                CodexPayload::ResponseItem(serde_json::from_value(payload_value.clone())
                    .map_err(D::Error::custom)?)
            }
            "event_msg" => {
                CodexPayload::EventMsg(serde_json::from_value(payload_value.clone())
                    .map_err(D::Error::custom)?)
            }
            "turn_context" => {
                CodexPayload::TurnContext(serde_json::from_value(payload_value.clone())
                    .map_err(D::Error::custom)?)
            }
            _ => {
                return Err(D::Error::custom(format!("Unknown message type: {}", message_type)));
            }
        };

        Ok(CodexMessage {
            timestamp,
            message_type,
            payload,
        })
    }
}

/// Codex payload wrapper
///
/// The payload structure is polymorphic based on parent message_type
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum CodexPayload {
    /// Session metadata (type: "session_meta")
    SessionMeta(SessionMetaPayload),
    /// Response items (type: "response_item")
    ResponseItem(ResponseItemPayload),
    /// Event messages (type: "event_msg")
    EventMsg(EventMsgPayload),
    /// Turn context (type: "turn_context")
    TurnContext(TurnContextPayload),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionMetaPayload {
    pub id: String,
    pub timestamp: String,
    pub cwd: String,
    pub originator: String,
    pub cli_version: Option<String>,
    pub git: Option<GitInfo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitInfo {
    pub commit_hash: Option<String>,
    pub branch: Option<String>,
    pub repository_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseItemPayload {
    #[serde(rename = "type")]
    pub item_type: String, // "message", "function_call", "function_call_output", "reasoning"
    #[serde(flatten)]
    pub data: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventMsgPayload {
    #[serde(rename = "type")]
    pub event_type: String, // "user_message", "agent_message", "token_count", "agent_reasoning"
    #[serde(flatten)]
    pub data: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnContextPayload {
    pub cwd: String,
    pub model: Option<String>,
    #[serde(flatten)]
    pub other: Value,
}

impl CodexMessage {
    /// Extract session ID from session_meta or use message UUID
    pub fn get_session_id(&self) -> Option<String> {
        match &self.payload {
            CodexPayload::SessionMeta(meta) => Some(meta.id.clone()),
            _ => None,
        }
    }

    /// Extract CWD from various payload types
    pub fn get_cwd(&self) -> Option<String> {
        match &self.payload {
            CodexPayload::SessionMeta(meta) => Some(meta.cwd.clone()),
            CodexPayload::TurnContext(ctx) => Some(ctx.cwd.clone()),
            _ => None,
        }
    }

    /// Extract git branch from session metadata
    pub fn get_git_branch(&self) -> Option<String> {
        match &self.payload {
            CodexPayload::SessionMeta(meta) => {
                meta.git.as_ref().and_then(|g| g.branch.clone())
            }
            _ => None,
        }
    }

    /// Extract version from session metadata
    pub fn get_version(&self) -> Option<String> {
        match &self.payload {
            CodexPayload::SessionMeta(meta) => meta.cli_version.clone(),
            _ => None,
        }
    }
}

impl ToCanonical for CodexMessage {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
        let session_id = self.get_session_id().unwrap_or_else(|| "unknown".to_string());
        let uuid = generate_uuid_from_codex(&self.timestamp, &session_id);

        match &self.payload {
            CodexPayload::ResponseItem(item) => {
                self.convert_response_item(item, &uuid, &session_id)
            }
            CodexPayload::EventMsg(event) => self.convert_event_msg(event, &uuid, &session_id),
            CodexPayload::SessionMeta(_) => {
                // Session meta becomes a meta message
                Ok(Some(CanonicalMessage {
                    uuid,
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::Meta,
                    session_id,
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: self.extract_git_branch(),
                    version: self.extract_version(),
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Text("Session started".to_string()),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::to_value(&self.payload)?),
                    is_meta: Some(true),
                    request_id: None,
                    tool_use_result: None,
                }))
            }
            CodexPayload::TurnContext(_) => {
                // Turn context is metadata - preserve full payload
                Ok(Some(CanonicalMessage {
                    uuid,
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::Meta,
                    session_id,
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: None,
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Text("".to_string()),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::to_value(&self.payload)?),
                    is_meta: Some(true),
                    request_id: None,
                    tool_use_result: None,
                }))
            }
        }
    }

    fn provider_name(&self) -> &str {
        "codex"
    }

    fn extract_cwd(&self) -> Option<String> {
        self.get_cwd()
    }

    fn extract_git_branch(&self) -> Option<String> {
        self.get_git_branch()
    }

    fn extract_version(&self) -> Option<String> {
        self.get_version()
    }
}

impl CodexMessage {
    fn convert_response_item(
        &self,
        item: &ResponseItemPayload,
        uuid: &str,
        session_id: &str,
    ) -> Result<Option<CanonicalMessage>> {
        match item.item_type.as_str() {
            "message" => {
                let role = item.data["role"]
                    .as_str()
                    .context("Missing role in message")?;
                let content = &item.data["content"];

                let content_text = if content.is_array() {
                    // Extract text from content array
                    content
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|c| c["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                } else if content.is_string() {
                    content.as_str().unwrap().to_string()
                } else {
                    String::new()
                };

                Ok(Some(CanonicalMessage {
                    uuid: uuid.to_string(),
                    timestamp: self.timestamp.clone(),
                    message_type: if role == "user" {
                        MessageType::User
                    } else {
                        MessageType::Assistant
                    },
                    session_id: session_id.to_string(),
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: self.extract_git_branch(),
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: role.to_string(),
                        content: ContentValue::Text(content_text),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "codex_type": "response_item",
                        "item_type": "message",
                    })),
                    is_meta: None,
                    request_id: None,
                    tool_use_result: None,
                }))
            }
            "function_call" => {
                let name = item.data["name"]
                    .as_str()
                    .context("Missing function name")?;
                let arguments = &item.data["arguments"];
                let call_id = item.data["call_id"].as_str().unwrap_or(uuid);

                let input: Value = if arguments.is_string() {
                    // Parse JSON string arguments
                    serde_json::from_str(arguments.as_str().unwrap()).unwrap_or(Value::Null)
                } else {
                    arguments.clone()
                };

                let block = ContentBlock::ToolUse {
                    id: call_id.to_string(),
                    name: name.to_string(),
                    input,
                };

                Ok(Some(CanonicalMessage {
                    uuid: uuid.to_string(),
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::Assistant,
                    session_id: session_id.to_string(),
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: self.extract_git_branch(),
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Structured(vec![block]),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "codex_type": "response_item",
                        "item_type": "function_call",
                    })),
                    is_meta: None,
                    request_id: None,
                    tool_use_result: None,
                }))
            }
            "function_call_output" => {
                let call_id = item.data["call_id"].as_str().unwrap_or(uuid);

                // Simple output extraction to match TypeScript
                let output = item.data["output"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();

                // Validate we have required data for tool_result
                // Don't create empty tool_result blocks (causes parsing issues)
                if call_id.is_empty() || output.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Tool result missing required fields: call_id='{}', output length={}",
                        call_id,
                        output.len()
                    ));
                }

                let block = ContentBlock::ToolResult {
                    tool_use_id: call_id.to_string(),
                    content: output,
                    is_error: Some(false),
                };

                Ok(Some(CanonicalMessage {
                    uuid: uuid.to_string(),
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::User,  // Tool results are USER messages
                    session_id: session_id.to_string(),
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: self.extract_git_branch(),
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "user".to_string(),  // Tool results have user role
                        content: ContentValue::Structured(vec![block]),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "codex_type": "response_item",
                        "item_type": "function_call_output",
                    })),
                    is_meta: None,
                    request_id: None,
                    tool_use_result: None,
                }))
            }
            "reasoning" => {
                // Codex reasoning becomes text content
                let summary = &item.data["summary"];
                let text = if summary.is_array() {
                    summary
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|s| s["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    String::new()
                };

                Ok(Some(CanonicalMessage {
                    uuid: uuid.to_string(),
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::Assistant,
                    session_id: session_id.to_string(),
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: self.extract_git_branch(),
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Text(text),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "codex_type": "response_item",
                        "item_type": "reasoning",
                    })),
                    is_meta: None,
                    request_id: None,
                    tool_use_result: None,
                }))
            }
            _ => {
                // Unknown type, preserve full payload for future analysis
                Ok(Some(CanonicalMessage {
                    uuid: uuid.to_string(),
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::Assistant,
                    session_id: session_id.to_string(),
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: self.extract_git_branch(),
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Text(String::new()),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "codex_type": "response_item",
                        "item_type": item.item_type,
                        "warning": "unknown response_item type",
                    })),
                    is_meta: None,
                    request_id: None,
                    tool_use_result: None,
                }))
            }
        }
    }

    fn convert_event_msg(
        &self,
        event: &EventMsgPayload,
        uuid: &str,
        session_id: &str,
    ) -> Result<Option<CanonicalMessage>> {
        match event.event_type.as_str() {
            "token_count" => {
                // Extract token usage from info
                let info = &event.data["info"];
                let usage = if !info.is_null() {
                    // Use last_token_usage (per-turn) not total_token_usage (cumulative)
                    let last = &info["last_token_usage"];
                    Some(TokenUsage {
                        input_tokens: last["input_tokens"].as_u64().map(|v| v as u32),
                        output_tokens: last["output_tokens"].as_u64().map(|v| v as u32),
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: last["cached_input_tokens"]
                            .as_u64()
                            .map(|v| v as u32),
                    })
                } else {
                    None
                };

                Ok(Some(CanonicalMessage {
                    uuid: uuid.to_string(),
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::Meta,
                    session_id: session_id.to_string(),
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: None,
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Text(String::new()),
                        model: None,
                        usage,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "codex_type": "event_msg",
                        "event_type": "token_count",
                    })),
                    is_meta: Some(true),
                    request_id: None,
                    tool_use_result: None,
                }))
            }
            "user_message" => {
                // Skip user_message event_msg - it duplicates response_item messages
                // User messages are already correctly captured by response_item with type="message" role="user"
                Ok(None)
            }
            "agent_message" => {
                // Skip agent_message event_msg - it duplicates response_item messages
                // Agent messages are already correctly captured by response_item with type="message" role="assistant"
                Ok(None)
            }
            "agent_reasoning" => {
                // Skip agent_reasoning event_msg - it duplicates response_item reasoning blocks
                // Agent reasoning is already correctly captured by response_item with type="reasoning"
                Ok(None)
            }
            _ => {
                // Unknown event type - preserve type for debugging
                Ok(Some(CanonicalMessage {
                    uuid: uuid.to_string(),
                    timestamp: self.timestamp.clone(),
                    message_type: MessageType::Meta,
                    session_id: session_id.to_string(),
                    provider: self.provider_name().to_string(),
                    cwd: self.extract_cwd(),
                    git_branch: None,
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Text(String::new()),
                        model: None,
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "codex_type": "event_msg",
                        "event_type": event.event_type,
                        "warning": "unknown event_msg type",
                    })),
                    is_meta: Some(true),
                    request_id: None,
                    tool_use_result: None,
                }))
            }
        }
    }
}

/// Generate a deterministic UUID from Codex timestamp and session ID
fn generate_uuid_from_codex(timestamp: &str, session_id: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    timestamp.hash(&mut hasher);
    session_id.hash(&mut hasher);
    let hash = hasher.finish();

    // Format as UUID-like string
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (hash >> 32) as u32,
        (hash >> 16) as u16,
        hash as u16,
        (hash >> 48) as u16,
        hash & 0xFFFFFFFFFFFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_meta() {
        let json = r#"{
            "timestamp": "2025-10-20T06:46:43.215Z",
            "type": "session_meta",
            "payload": {
                "id": "019a005e-c8fc-7512-8e78-c2322cbf0875",
                "timestamp": "2025-10-20T06:46:43.196Z",
                "cwd": "/Users/cliftonc/work/guideai",
                "originator": "codex_cli_rs",
                "cli_version": "0.45.0",
                "git": {
                    "commit_hash": "77a017",
                    "branch": "main",
                    "repository_url": "git@github.com:guideai-dev/guideai.git"
                }
            }
        }"#;

        let msg: CodexMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_type, "session_meta");
        assert_eq!(msg.get_session_id(), Some("019a005e-c8fc-7512-8e78-c2322cbf0875".to_string()));
        assert_eq!(msg.get_cwd(), Some("/Users/cliftonc/work/guideai".to_string()));
        assert_eq!(msg.get_git_branch(), Some("main".to_string()));
    }

    #[test]
    fn test_convert_user_message() {
        let json = r#"{
            "timestamp": "2025-10-20T06:46:47.990Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "Can you review the claude.md?"}]
            }
        }"#;

        let msg: CodexMessage = serde_json::from_str(json).unwrap();
        let canonical = msg.to_canonical().unwrap().unwrap();

        assert_eq!(canonical.message_type, MessageType::User);
        assert_eq!(canonical.message.role, "user");

        match canonical.message.content {
            ContentValue::Text(text) => assert_eq!(text, "Can you review the claude.md?"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_function_call() {
        let json = r#"{
            "timestamp": "2025-10-20T06:46:51.694Z",
            "type": "response_item",
            "payload": {
                "type": "function_call",
                "name": "shell",
                "arguments": "{\"command\":[\"bash\",\"-lc\",\"ls\"]}",
                "call_id": "call_XhFBtWxG4rvlC5GE7r6MIwDO"
            }
        }"#;

        let msg: CodexMessage = serde_json::from_str(json).unwrap();
        let canonical = msg.to_canonical().unwrap().unwrap();

        match canonical.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call_XhFBtWxG4rvlC5GE7r6MIwDO");
                        assert_eq!(name, "shell");
                    }
                    _ => panic!("Expected tool_use block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_uuid_generation() {
        let uuid1 = generate_uuid_from_codex("2025-01-01T00:00:00.000Z", "session-1");
        let uuid2 = generate_uuid_from_codex("2025-01-01T00:00:00.000Z", "session-1");
        let uuid3 = generate_uuid_from_codex("2025-01-01T00:00:01.000Z", "session-1");

        // Same inputs should generate same UUID
        assert_eq!(uuid1, uuid2);

        // Different inputs should generate different UUIDs
        assert_ne!(uuid1, uuid3);
    }
}
