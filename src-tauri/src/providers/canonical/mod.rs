use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod converter;

#[cfg(test)]
mod tests;

/// Canonical JSONL message format (based on Claude Code)
///
/// This is the unified format that all providers convert to for consistent processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalMessage {
    /// Unique message identifier
    pub uuid: String,

    /// ISO 8601 timestamp
    pub timestamp: String,

    /// Message type
    #[serde(rename = "type")]
    pub message_type: MessageType,

    /// Session identifier
    pub session_id: String,

    /// Provider name (e.g., "claude-code", "gemini-code", "codex")
    pub provider: String,

    /// Current working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// Git branch name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,

    /// Provider version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Parent message UUID (for threading)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_uuid: Option<String>,

    /// Whether this is a sidechain message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_sidechain: Option<bool>,

    /// User type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_type: Option<String>,

    /// Message content
    pub message: MessageContent,

    /// Optional provider-specific metadata
    /// Preserves provider-specific fields that don't fit in canonical schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<Value>,

    /// Whether this is a meta message (system events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_meta: Option<bool>,

    /// Request ID from provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Tool use result data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_result: Option<Value>,
}

/// Message type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    User,
    Assistant,
    Meta,
}

/// Message content structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    /// Role: "user" or "assistant"
    pub role: String,

    /// Content: either plain text or structured content blocks
    pub content: ContentValue,

    /// Model name (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Token usage information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Content can be either plain text or structured blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentValue {
    /// Simple text content
    Text(String),
    /// Structured content blocks (text, tool_use, tool_result)
    Structured(Vec<ContentBlock>),
}

/// Content block types for structured messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text block
    Text {
        text: String,
    },
    /// Tool invocation block
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Tool execution result block
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    /// Thinking/reasoning block (extended thinking from Claude, thoughts from Gemini, etc.)
    Thinking {
        thinking: String,
    },
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens consumed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,

    /// Output tokens generated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,

    /// Cache creation tokens (prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,

    /// Cache read tokens (prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
}

impl CanonicalMessage {
    /// Create a simple text message
    #[allow(dead_code)]
    pub fn new_text_message(
        uuid: String,
        timestamp: String,
        message_type: MessageType,
        session_id: String,
        provider: String,
        role: String,
        text: String,
    ) -> Self {
        Self {
            uuid,
            timestamp,
            message_type,
            session_id,
            provider,
            cwd: None,
            git_branch: None,
            version: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: Some("external".to_string()),
            message: MessageContent {
                role,
                content: ContentValue::Text(text),
                model: None,
                usage: None,
            },
            provider_metadata: None,
            is_meta: None,
            request_id: None,
            tool_use_result: None,
        }
    }

    /// Create a structured message with content blocks
    #[allow(dead_code)]
    pub fn new_structured_message(
        uuid: String,
        timestamp: String,
        message_type: MessageType,
        session_id: String,
        provider: String,
        role: String,
        blocks: Vec<ContentBlock>,
    ) -> Self {
        Self {
            uuid,
            timestamp,
            message_type,
            session_id,
            provider,
            cwd: None,
            git_branch: None,
            version: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: Some("external".to_string()),
            message: MessageContent {
                role,
                content: ContentValue::Structured(blocks),
                model: None,
                usage: None,
            },
            provider_metadata: None,
            is_meta: None,
            request_id: None,
            tool_use_result: None,
        }
    }
}
