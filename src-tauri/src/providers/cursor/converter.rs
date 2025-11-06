/// Converter from Cursor protobuf format to canonical JSONL

use super::protobuf::{CursorBlob, ContentBlock as CursorContentBlock, CursorMessage};
use crate::providers::canonical::{
    CanonicalMessage, ContentBlock, ContentValue, MessageContent, MessageType,
};
use crate::providers::canonical::converter::ToCanonical;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

/// Wrapper for CursorMessage with session metadata for timestamp calculation
///
/// Since Cursor doesn't store per-message timestamps, we calculate them based on:
/// - Session `createdAt` timestamp (from meta table)
/// - Message index/ordering (from database rowid)
/// - 1 second increment per message for realistic timing
pub struct CursorMessageWithRaw<'a> {
    pub message: &'a CursorMessage,
    pub raw_data: &'a [u8],
    pub session_created_at: i64, // Unix timestamp in milliseconds
    pub message_index: usize,     // Index in the session for timestamp calculation
}

impl<'a> CursorMessageWithRaw<'a> {
    pub fn new(
        message: &'a CursorMessage,
        raw_data: &'a [u8],
        session_created_at: i64,
        message_index: usize,
    ) -> Self {
        Self {
            message,
            raw_data,
            session_created_at,
            message_index,
        }
    }

    /// Calculate timestamp for this message based on session creation time and message index
    fn calculate_timestamp(&self) -> String {
        let base_timestamp = DateTime::from_timestamp_millis(self.session_created_at)
            .unwrap_or_else(Utc::now);
        let message_timestamp = base_timestamp + chrono::Duration::seconds(self.message_index as i64);
        message_timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// Convert to multiple canonical messages (split by content block type)
    pub fn to_canonical_split(&self) -> Result<Vec<CanonicalMessage>> {
        if let Some(msg) = self.to_canonical()? {
            Ok(vec![msg])
        } else {
            Ok(vec![])
        }
    }
}

impl<'a> ToCanonical for CursorMessageWithRaw<'a> {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
        match self.message {
            CursorMessage::Protobuf(blob) => {
                blob.to_canonical_with_timestamp_and_raw(&self.calculate_timestamp(), self.raw_data)
            }
            CursorMessage::Json(json_msg) => {
                convert_json_message_with_timestamp(json_msg, &self.calculate_timestamp())
            }
        }
    }

    fn provider_name(&self) -> &str {
        "cursor"
    }

    fn extract_cwd(&self) -> Option<String> {
        None
    }

    fn extract_git_branch(&self) -> Option<String> {
        None
    }

    fn extract_version(&self) -> Option<String> {
        None
    }
}

impl ToCanonical for CursorBlob {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
        let timestamp = Utc::now().to_rfc3339();
        self.to_canonical_with_timestamp_and_raw(&timestamp, &[])
    }

    fn provider_name(&self) -> &str {
        "cursor"
    }

    fn extract_cwd(&self) -> Option<String> {
        None
    }

    fn extract_git_branch(&self) -> Option<String> {
        None
    }

    fn extract_version(&self) -> Option<String> {
        None
    }
}

impl CursorBlob {
    fn to_canonical_with_timestamp_and_raw(&self, timestamp: &str, raw_data: &[u8]) -> Result<Option<CanonicalMessage>> {
        let content_text = self.get_content_with_fallback(raw_data);
        if content_text.is_empty() && !self.is_complex() {
            return Ok(None);
        }

        let role = self.get_role();
        let message_type = match role.as_str() {
            "user" => MessageType::User,
            "assistant" => MessageType::Assistant,
            _ => MessageType::Meta,
        };

        let content = if self.is_complex() {
            self.build_structured_content()?
        } else {
            ContentValue::Text(content_text)
        };

        let model = self
            .parse_complex()
            .and_then(|c| Some(c.role.clone()))
            .filter(|r| r == "assistant")
            .map(|_| "default".to_string());

        Ok(Some(CanonicalMessage {
            uuid: self.uuid.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            timestamp: timestamp.to_string(),
            message_type,
            session_id: String::new(),
            provider: "cursor".to_string(),
            cwd: None,
            git_branch: None,
            version: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            message: MessageContent {
                role: role.clone(),
                content,
                model,
                usage: None, // Cursor doesn't expose token usage in blobs
            },
            provider_metadata: Some(self.build_metadata()),
            is_meta: None,
            request_id: None,
            tool_use_result: None,
        }))
    }

    /// Build structured content from complex message data
    fn build_structured_content(&self) -> Result<ContentValue> {
        let complex = self
            .parse_complex()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse complex message"))?;

        let mut blocks = Vec::new();

        for cursor_block in complex.content {
            match cursor_block {
                CursorContentBlock::Text { text } => {
                    blocks.push(ContentBlock::Text { text });
                }
                CursorContentBlock::ToolCall {
                    tool_call_id,
                    tool_name,
                    args,
                } => {
                    blocks.push(ContentBlock::ToolUse {
                        id: tool_call_id,
                        name: tool_name,
                        input: args,
                    });
                }
                CursorContentBlock::ToolResult {
                    tool_call_id,
                    output,
                    is_error,
                } => {
                    blocks.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call_id,
                        content: output,
                        is_error: Some(is_error),
                    });
                }
                CursorContentBlock::RedactedReasoning { data } => {
                    blocks.push(ContentBlock::Thinking {
                        thinking: format!("[Redacted reasoning: {} bytes]", data.len()),
                    });
                }
            }
        }

        Ok(ContentValue::Structured(blocks))
    }

    /// Build provider-specific metadata
    fn build_metadata(&self) -> Value {
        let mut metadata = json!({});

        if let Some(ref meta) = self.metadata {
            if !meta.is_empty() {
                metadata["metadata"] = json!(meta);
            }
        }

        if let Some(ref _complex) = self.complex_data {
            metadata["has_complex_data"] = json!(true);

            // Try to parse and extract interesting fields
            if let Some(parsed) = self.parse_complex() {
                metadata["message_id"] = json!(parsed.id);
            }
        }

        if self.additional_content.is_some() {
            metadata["has_additional_content"] = json!(true);
        }

        if self.blob_references.is_some() {
            metadata["has_blob_references"] = json!(true);
        }

        metadata
    }
}

/// Convert JSON message with timestamp
fn convert_json_message_with_timestamp(
    json_msg: &super::protobuf::JsonMessage,
    timestamp: &str,
) -> Result<Option<CanonicalMessage>> {
    let message_type = match json_msg.role.as_str() {
        "user" => MessageType::User,
        "assistant" => MessageType::Assistant,
        "system" => MessageType::Meta,
        "tool" => MessageType::User,
        _ => MessageType::Meta,
    };

    let content = if json_msg.content.is_string() {
        ContentValue::Text(json_msg.content.as_str().unwrap_or("").to_string())
    } else if json_msg.content.is_array() {
        let content_blocks = json_msg.content.as_array().unwrap();
        let mut blocks = Vec::new();

        for block in content_blocks {
            if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            blocks.push(ContentBlock::Text {
                                text: text.to_string(),
                            });
                        }
                    }
                    "tool_use" => {
                        if let (Some(id), Some(name), Some(input)) = (
                            block.get("id").and_then(|i| i.as_str()),
                            block.get("name").and_then(|n| n.as_str()),
                            block.get("input"),
                        ) {
                            blocks.push(ContentBlock::ToolUse {
                                id: id.to_string(),
                                name: name.to_string(),
                                input: input.clone(),
                            });
                        }
                    }
                    "tool_result" => {
                        if let (Some(tool_use_id), Some(content)) = (
                            block.get("tool_use_id").and_then(|i| i.as_str()),
                            block.get("content"),
                        ) {
                            let content_str = if content.is_string() {
                                content.as_str().unwrap().to_string()
                            } else {
                                content.to_string()
                            };

                            blocks.push(ContentBlock::ToolResult {
                                tool_use_id: tool_use_id.to_string(),
                                content: content_str,
                                is_error: block.get("is_error").and_then(|e| e.as_bool()),
                            });
                        }
                    }
                    _ => {
                        blocks.push(ContentBlock::Text {
                            text: format!("[Unknown block type: {}]", block_type),
                        });
                    }
                }
            }
        }

        if blocks.is_empty() {
            ContentValue::Text(String::new())
        } else {
            ContentValue::Structured(blocks)
        }
    } else {
        ContentValue::Text(json_msg.content.to_string())
    };

    Ok(Some(CanonicalMessage {
        uuid: if json_msg.id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            json_msg.id.clone()
        },
        timestamp: timestamp.to_string(),
        message_type,
        session_id: String::new(),
        provider: "cursor".to_string(),
        cwd: None,
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: if json_msg.role == "user" {
            Some("external".to_string())
        } else {
            None
        },
        message: MessageContent {
            role: json_msg.role.clone(),
            content,
            model: None,
            usage: None,
        },
        provider_metadata: Some(json!({
            "format": "json",
            "original_content_type": if json_msg.content.is_string() {
                "string"
            } else if json_msg.content.is_array() {
                "array"
            } else {
                "object"
            },
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_message_conversion() {
        let blob = CursorBlob {
            content: "Test message".to_string(),
            uuid: "test-uuid".to_string(),
            metadata: String::new(),
            complex_data: None,
            additional_content: None,
            blob_references: None,
        };

        let canonical = blob.to_canonical().unwrap().unwrap();

        assert_eq!(canonical.provider, "cursor");
        assert_eq!(canonical.uuid, "test-uuid");

        match canonical.message.content {
            ContentValue::Text(text) => assert_eq!(text, "Test message"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_skip_empty_messages() {
        let blob = CursorBlob {
            content: String::new(),
            uuid: String::new(),
            metadata: String::new(),
            complex_data: None,
            additional_content: None,
            blob_references: None,
        };

        let result = blob.to_canonical().unwrap();
        assert!(result.is_none(), "Empty messages should be skipped");
    }
}
