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
        match self.message {
            CursorMessage::Protobuf(blob) => {
                if blob.is_complex() {
                    // Split complex messages into separate canonical messages
                    split_complex_message(blob, &self.calculate_timestamp(), self.raw_data)
                } else {
                    // Simple messages stay as single message
                    if let Some(msg) = self.to_canonical()? {
                        Ok(vec![msg])
                    } else {
                        Ok(vec![])
                    }
                }
            }
            CursorMessage::Json(json_msg) => {
                // JSON messages may also need splitting if they have mixed content
                if let Some(msg) = self.to_canonical()? {
                    // Check if the message has structured content with mixed types
                    if let ContentValue::Structured(ref blocks) = msg.message.content {
                        if should_split_blocks(blocks) {
                            // Split into separate messages
                            split_json_message(json_msg, &self.calculate_timestamp(), blocks)
                        } else {
                            Ok(vec![msg])
                        }
                    } else {
                        Ok(vec![msg])
                    }
                } else {
                    Ok(vec![])
                }
            }
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

/// Check if content blocks should be split into separate messages
fn should_split_blocks(blocks: &[ContentBlock]) -> bool {
    // Split if we have tool_use, tool_result, or thinking mixed with other content
    let has_tool_use = blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. }));
    let has_tool_result = blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }));
    let has_thinking = blocks.iter().any(|b| matches!(b, ContentBlock::Thinking { .. }));
    let has_text = blocks.iter().any(|b| matches!(b, ContentBlock::Text { .. }));

    // Split if we have multiple different types
    let type_count = [has_tool_use, has_tool_result, has_thinking, has_text]
        .iter()
        .filter(|&&x| x)
        .count();

    type_count > 1
}

/// Split a JSON message with mixed content into separate canonical messages
fn split_json_message(
    json_msg: &super::protobuf::JsonMessage,
    timestamp: &str,
    blocks: &[ContentBlock],
) -> Result<Vec<CanonicalMessage>> {
    let mut messages = Vec::new();
    let mut text_blocks = Vec::new();
    let mut thinking_blocks = Vec::new();

    // Determine message type and role
    let base_message_type = match json_msg.role.as_str() {
        "user" => MessageType::User,
        "assistant" => MessageType::Assistant,
        "system" => MessageType::Meta,
        "tool" => MessageType::User,
        _ => MessageType::Meta,
    };

    let base_role = match json_msg.role.as_str() {
        "tool" => "user".to_string(),
        _ => json_msg.role.clone(),
    };

    for block in blocks {
        match block {
            ContentBlock::Text { text } => {
                if !text.is_empty() {
                    text_blocks.push(text.clone());
                }
            }
            ContentBlock::ToolUse { id, name, input } => {
                // Flush text first
                if !text_blocks.is_empty() {
                    messages.push(create_json_text_message(
                        json_msg,
                        timestamp,
                        MessageType::Assistant,
                        "assistant",
                        text_blocks.join("\n"),
                    )?);
                    text_blocks.clear();
                }

                // Create tool use message
                let block = ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                };
                messages.push(create_json_structured_message(
                    json_msg,
                    timestamp,
                    MessageType::Assistant,
                    "assistant",
                    vec![block],
                )?);
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                // Flush text first
                if !text_blocks.is_empty() {
                    messages.push(create_json_text_message(
                        json_msg,
                        timestamp,
                        MessageType::Assistant,
                        "assistant",
                        text_blocks.join("\n"),
                    )?);
                    text_blocks.clear();
                }

                // Create tool result message (USER message!)
                let block = ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: content.clone(),
                    is_error: *is_error,
                };
                messages.push(create_json_structured_message(
                    json_msg,
                    timestamp,
                    MessageType::User,
                    "user",
                    vec![block],
                )?);
            }
            ContentBlock::Thinking { thinking } => {
                thinking_blocks.push(ContentBlock::Thinking {
                    thinking: thinking.clone(),
                });
            }
        }
    }

    // Flush remaining text
    if !text_blocks.is_empty() {
        messages.push(create_json_text_message(
            json_msg,
            timestamp,
            base_message_type,
            &base_role,
            text_blocks.join("\n"),
        )?);
    }

    // Flush thinking
    if !thinking_blocks.is_empty() {
        messages.push(create_json_structured_message(
            json_msg,
            timestamp,
            MessageType::Assistant,
            "assistant",
            thinking_blocks,
        )?);
    }

    Ok(messages)
}

/// Helper to create a text message from JSON message
fn create_json_text_message(
    json_msg: &super::protobuf::JsonMessage,
    timestamp: &str,
    message_type: MessageType,
    role: &str,
    text: String,
) -> Result<CanonicalMessage> {
    let unique_uuid = format!("{}-{}", json_msg.id, uuid::Uuid::new_v4());

    Ok(CanonicalMessage {
        uuid: unique_uuid,
        timestamp: timestamp.to_string(),
        message_type,
        session_id: String::new(),
        provider: "cursor".to_string(),
        cwd: None,
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: if role == "user" {
            Some("external".to_string())
        } else {
            None
        },
        message: MessageContent {
            role: role.to_string(),
            content: ContentValue::Text(text),
            model: if role == "assistant" {
                Some("default".to_string())
            } else {
                None
            },
            usage: None,
        },
        provider_metadata: Some(json!({
            "format": "json",
            "original_content_type": "array"
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Helper to create a structured message from JSON message
fn create_json_structured_message(
    json_msg: &super::protobuf::JsonMessage,
    timestamp: &str,
    message_type: MessageType,
    role: &str,
    blocks: Vec<ContentBlock>,
) -> Result<CanonicalMessage> {
    let unique_uuid = format!("{}-{}", json_msg.id, uuid::Uuid::new_v4());

    Ok(CanonicalMessage {
        uuid: unique_uuid,
        timestamp: timestamp.to_string(),
        message_type,
        session_id: String::new(),
        provider: "cursor".to_string(),
        cwd: None,
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: if role == "user" {
            Some("external".to_string())
        } else {
            None
        },
        message: MessageContent {
            role: role.to_string(),
            content: ContentValue::Structured(blocks),
            model: if role == "assistant" {
                Some("default".to_string())
            } else {
                None
            },
            usage: None,
        },
        provider_metadata: Some(json!({
            "format": "json",
            "original_content_type": "array"
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Split a complex message into multiple canonical messages based on content block types
fn split_complex_message(
    blob: &CursorBlob,
    timestamp: &str,
    _raw_data: &[u8],
) -> Result<Vec<CanonicalMessage>> {
    let complex = blob
        .parse_complex()
        .ok_or_else(|| anyhow::anyhow!("Failed to parse complex message"))?;

    let mut messages = Vec::new();
    let mut text_blocks = Vec::new();
    let mut thinking_blocks = Vec::new();

    for cursor_block in complex.content {
        match cursor_block {
            // Accumulate text blocks
            CursorContentBlock::Text { text } => {
                if !text.is_empty() {
                    text_blocks.push(text);
                }
            }

            // Tool use goes in its own assistant message
            CursorContentBlock::ToolCall {
                tool_call_id,
                tool_name,
                args,
            } => {
                // Flush any accumulated text first
                if !text_blocks.is_empty() {
                    messages.push(create_text_message(
                        blob,
                        timestamp,
                        MessageType::Assistant,
                        "assistant",
                        text_blocks.join("\n"),
                    )?);
                    text_blocks.clear();
                }

                // Create tool use message
                let block = ContentBlock::ToolUse {
                    id: tool_call_id,
                    name: tool_name,
                    input: args,
                };
                messages.push(create_structured_message(
                    blob,
                    timestamp,
                    MessageType::Assistant,
                    "assistant",
                    vec![block],
                )?);
            }

            // Tool result goes in its own USER message (CRITICAL!)
            CursorContentBlock::ToolResult {
                tool_call_id,
                output,
                is_error,
            } => {
                // Flush any accumulated text first
                if !text_blocks.is_empty() {
                    messages.push(create_text_message(
                        blob,
                        timestamp,
                        MessageType::Assistant,
                        "assistant",
                        text_blocks.join("\n"),
                    )?);
                    text_blocks.clear();
                }

                // Ensure content is not empty (canonical requires non-empty)
                let content = if output.is_empty() {
                    "(no output)".to_string()
                } else {
                    output
                };

                // Create tool result message (USER message!)
                let block = ContentBlock::ToolResult {
                    tool_use_id: tool_call_id,
                    content,
                    is_error: Some(is_error),
                };
                messages.push(create_structured_message(
                    blob,
                    timestamp,
                    MessageType::User, // ← USER!
                    "user",            // ← user role!
                    vec![block],
                )?);
            }

            // Thinking blocks accumulate
            CursorContentBlock::RedactedReasoning { data } => {
                thinking_blocks.push(ContentBlock::Thinking {
                    thinking: format!("[Redacted reasoning: {} bytes]", data.len()),
                });
            }
        }
    }

    // Flush remaining text blocks
    if !text_blocks.is_empty() {
        messages.push(create_text_message(
            blob,
            timestamp,
            MessageType::Assistant,
            "assistant",
            text_blocks.join("\n"),
        )?);
    }

    // Flush thinking blocks
    if !thinking_blocks.is_empty() {
        messages.push(create_structured_message(
            blob,
            timestamp,
            MessageType::Assistant,
            "assistant",
            thinking_blocks,
        )?);
    }

    // If no messages were created, return empty vec
    Ok(messages)
}

/// Helper to create a text-only message
fn create_text_message(
    blob: &CursorBlob,
    timestamp: &str,
    message_type: MessageType,
    role: &str,
    text: String,
) -> Result<CanonicalMessage> {
    // Generate unique UUID for each split message
    // Use original UUID as base, but append suffix to make unique
    let base_uuid = blob.uuid.as_deref().unwrap_or("unknown");
    let unique_uuid = format!("{}-{}", base_uuid, uuid::Uuid::new_v4());

    Ok(CanonicalMessage {
        uuid: unique_uuid,
        timestamp: timestamp.to_string(),
        message_type,
        session_id: String::new(),
        provider: "cursor".to_string(),
        cwd: None,
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: if role == "user" {
            Some("external".to_string())
        } else {
            None
        },
        message: MessageContent {
            role: role.to_string(),
            content: ContentValue::Text(text),
            model: if role == "assistant" {
                Some("default".to_string())
            } else {
                None
            },
            usage: None,
        },
        provider_metadata: Some(blob.build_metadata()),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Helper to create a structured message with content blocks
fn create_structured_message(
    blob: &CursorBlob,
    timestamp: &str,
    message_type: MessageType,
    role: &str,
    blocks: Vec<ContentBlock>,
) -> Result<CanonicalMessage> {
    // Generate unique UUID for each split message
    let base_uuid = blob.uuid.as_deref().unwrap_or("unknown");
    let unique_uuid = format!("{}-{}", base_uuid, uuid::Uuid::new_v4());

    Ok(CanonicalMessage {
        uuid: unique_uuid,
        timestamp: timestamp.to_string(),
        message_type,
        session_id: String::new(),
        provider: "cursor".to_string(),
        cwd: None,
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: if role == "user" {
            Some("external".to_string())
        } else {
            None
        },
        message: MessageContent {
            role: role.to_string(),
            content: ContentValue::Structured(blocks),
            model: if role == "assistant" {
                Some("default".to_string())
            } else {
                None
            },
            usage: None,
        },
        provider_metadata: Some(blob.build_metadata()),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
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
            .parse_complex().map(|c| c.role.clone())
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
                    // Ensure content is not empty (canonical requires non-empty)
                    let content = if output.is_empty() {
                        "(no output)".to_string()
                    } else {
                        output
                    };

                    blocks.push(ContentBlock::ToolResult {
                        tool_use_id: tool_call_id,
                        content,
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

/// Parse text content that may contain <think> tags
/// Extracts thinking sections and creates separate blocks
fn parse_text_with_thinking(text: &str, blocks: &mut Vec<ContentBlock>) {
    let mut remaining = text;

    while let Some(think_start) = remaining.find("<think>") {
        // Add any text before <think> tag
        if think_start > 0 {
            let before_text = remaining[..think_start].trim();
            if !before_text.is_empty() {
                blocks.push(ContentBlock::Text {
                    text: before_text.to_string(),
                });
            }
        }

        // Find closing tag
        if let Some(think_end_start) = remaining[think_start..].find("</think>") {
            let think_end = think_start + think_end_start;
            let think_content = &remaining[think_start + 7..think_end]; // Skip "<think>"

            blocks.push(ContentBlock::Thinking {
                thinking: think_content.trim().to_string(),
            });

            // Continue with text after </think>
            remaining = &remaining[think_end + 8..]; // Skip "</think>"
        } else {
            // No closing tag, treat rest as text
            blocks.push(ContentBlock::Text {
                text: remaining.to_string(),
            });
            return;
        }
    }

    // Add any remaining text
    let remaining_text = remaining.trim();
    if !remaining_text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: remaining_text.to_string(),
        });
    }
}

/// Parse content array (handles both direct arrays and experimental_content format)
fn parse_content_array(content: &serde_json::Value, role: &str, message_id: &str) -> ContentValue {
    let content_blocks_value = if let Some(array) = content.as_array() {
        // Check if this is experimental_content format
        if !array.is_empty() && array[0].get("experimental_content").is_some() {
            // Extract experimental_content
            if let Some(exp_content) = array[0].get("experimental_content") {
                exp_content
            } else {
                content
            }
        } else {
            content
        }
    } else {
        content
    };

    let content_blocks = if let Some(arr) = content_blocks_value.as_array() {
        arr
    } else {
        return ContentValue::Text(String::new());
    };

    let mut blocks = Vec::new();

    //  For tool role messages, create tool_result blocks from the text content
    if role == "tool" {
        // Tool messages contain the result of a tool execution
        // Extract all text and create a single tool_result block
        let mut text_parts = Vec::new();
        for block in content_blocks {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                text_parts.push(text.to_string());
            }
        }

        if !text_parts.is_empty() {
            let content = text_parts.join("\n");
            let final_content = if content.is_empty() {
                "(no output)".to_string()
            } else {
                content
            };

            blocks.push(ContentBlock::ToolResult {
                tool_use_id: message_id.to_string(),
                content: final_content,
                is_error: None,
            });
        }

        return if blocks.is_empty() {
            ContentValue::Text(String::new())
        } else {
            ContentValue::Structured(blocks)
        };
    }

    for block in content_blocks {
        if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        // Check if text contains <think> tags - extract and create separate blocks
                        parse_text_with_thinking(text, &mut blocks);
                    }
                }
                "reasoning" => {
                    // Cursor's reasoning blocks map to thinking blocks
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        blocks.push(ContentBlock::Thinking {
                            thinking: text.to_string(),
                        });
                    }
                }
                "tool-call" => {
                    // Cursor's tool-call format uses toolCallId, toolName, args
                    if let (Some(id), Some(name), Some(args)) = (
                        block.get("toolCallId").and_then(|i| i.as_str()),
                        block.get("toolName").and_then(|n| n.as_str()),
                        block.get("args"),
                    ) {
                        blocks.push(ContentBlock::ToolUse {
                            id: id.to_string(),
                            name: name.to_string(),
                            input: args.clone(),
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
                "tool_result" | "tool-result" => {
                    // Handle both "tool_result" and "tool-result" formats
                    if let (Some(tool_use_id), Some(content)) = (
                        block.get("tool_use_id").and_then(|i| i.as_str()),
                        block.get("content"),
                    ) {
                        let content_str = if content.is_string() {
                            content.as_str().unwrap().to_string()
                        } else {
                            content.to_string()
                        };

                        // Ensure content is not empty
                        let final_content = if content_str.is_empty() {
                            "(no output)".to_string()
                        } else {
                            content_str
                        };

                        blocks.push(ContentBlock::ToolResult {
                            tool_use_id: tool_use_id.to_string(),
                            content: final_content,
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
    } else if blocks.len() == 1 && matches!(blocks[0], ContentBlock::Text { .. }) {
        // Single text block - return as text
        if let ContentBlock::Text { text } = &blocks[0] {
            ContentValue::Text(text.clone())
        } else {
            ContentValue::Structured(blocks)
        }
    } else {
        ContentValue::Structured(blocks)
    }
}

/// Convert JSON message with timestamp
fn convert_json_message_with_timestamp(
    json_msg: &super::protobuf::JsonMessage,
    timestamp: &str,
) -> Result<Option<CanonicalMessage>> {
    // Map role to message type
    // IMPORTANT: "tool" role messages are tool results and must be USER messages
    let message_type = match json_msg.role.as_str() {
        "user" => MessageType::User,
        "assistant" => MessageType::Assistant,
        "system" => MessageType::Meta,
        "tool" => MessageType::User, // tool results are user messages
        _ => MessageType::Meta,
    };

    // Map role for canonical format
    // IMPORTANT: "tool" role must be converted to "user" role
    let canonical_role = match json_msg.role.as_str() {
        "tool" => "user".to_string(), // Convert tool to user
        _ => json_msg.role.clone(),
    };

    let content = if json_msg.content.is_string() {
        // Try to parse as JSON first (for tool messages with JSON string content)
        let content_str = json_msg.content.as_str().unwrap_or("");
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content_str) {
            if parsed.is_array() {
                // Parse the JSON array content, pass message ID for tool results
                parse_content_array(&parsed, &json_msg.role, &json_msg.id)
            } else {
                // Not an array, treat as plain text
                ContentValue::Text(content_str.to_string())
            }
        } else {
            // Not JSON, treat as plain text
            ContentValue::Text(content_str.to_string())
        }
    } else if json_msg.content.is_array() {
        // Direct array content, pass message ID for tool results
        parse_content_array(&json_msg.content, &json_msg.role, &json_msg.id)
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
        user_type: if canonical_role == "user" {
            Some("external".to_string())
        } else {
            None
        },
        message: MessageContent {
            role: canonical_role,
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
    use serde_json::json;

    #[test]
    fn test_simple_message_conversion() {
        use super::super::protobuf::ContentWrapper;

        let blob = CursorBlob {
            content_wrapper: Some(ContentWrapper {
                text: Some("Test message".to_string()),
            }),
            uuid: Some("test-uuid".to_string()),
            metadata: Some(String::new()),
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
            content_wrapper: None,
            uuid: None,
            metadata: Some(String::new()),
            complex_data: None,
            additional_content: None,
            blob_references: None,
        };

        let result = blob.to_canonical().unwrap();
        assert!(result.is_none(), "Empty messages should be skipped");
    }

    #[test]
    fn test_split_complex_message_with_tool_call_and_result() {
        // Create a complex message with tool call and result
        let complex_data = json!({
            "id": "msg-123",
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "I'll run the command now."
                },
                {
                    "type": "tool-call",
                    "toolCallId": "call-abc123",
                    "toolName": "bash",
                    "args": {"command": "ls -la"}
                },
                {
                    "type": "tool-result",
                    "toolCallId": "call-abc123",
                    "output": "total 48\ndrwxr-xr-x  12 user  staff  384 Jan  1 00:00 .\n...",
                    "is_error": false
                }
            ]
        });

        let blob = CursorBlob {
            content_wrapper: None,
            uuid: Some("test-uuid".to_string()),
            metadata: Some(String::new()),
            complex_data: Some(complex_data.to_string()),
            additional_content: None,
            blob_references: None,
        };

        let timestamp = "2025-01-01T00:00:00.000Z";
        let messages = split_complex_message(&blob, timestamp, &[]).unwrap();

        // Should create 3 messages:
        // 1. Assistant message with text
        // 2. Assistant message with tool_use
        // 3. User message with tool_result
        assert_eq!(messages.len(), 3, "Should split into 3 messages");

        // Check first message (text)
        assert_eq!(messages[0].message_type, MessageType::Assistant);
        assert_eq!(messages[0].message.role, "assistant");
        match &messages[0].message.content {
            ContentValue::Text(text) => assert_eq!(text, "I'll run the command now."),
            _ => panic!("Expected text content in first message"),
        }

        // Check second message (tool_use)
        assert_eq!(messages[1].message_type, MessageType::Assistant);
        assert_eq!(messages[1].message.role, "assistant");
        match &messages[1].message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call-abc123");
                        assert_eq!(name, "bash");
                    }
                    _ => panic!("Expected tool_use block"),
                }
            }
            _ => panic!("Expected structured content in second message"),
        }

        // Check third message (tool_result) - CRITICAL: Must be USER message
        assert_eq!(messages[2].message_type, MessageType::User, "Tool result must be USER message");
        assert_eq!(messages[2].message.role, "user", "Tool result must have user role");
        match &messages[2].message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                        assert_eq!(tool_use_id, "call-abc123");
                        assert!(content.contains("total 48"));
                        assert_eq!(*is_error, Some(false));
                    }
                    _ => panic!("Expected tool_result block"),
                }
            }
            _ => panic!("Expected structured content in third message"),
        }
    }

    #[test]
    fn test_tool_result_has_user_type_and_role() {
        let complex_data = json!({
            "id": "msg-123",
            "role": "assistant",
            "content": [
                {
                    "type": "tool-result",
                    "toolCallId": "call-xyz",
                    "output": "Success",
                    "is_error": false
                }
            ]
        });

        let blob = CursorBlob {
            content_wrapper: None,
            uuid: Some("test-uuid".to_string()),
            metadata: Some(String::new()),
            complex_data: Some(complex_data.to_string()),
            additional_content: None,
            blob_references: None,
        };

        let timestamp = "2025-01-01T00:00:00.000Z";
        let messages = split_complex_message(&blob, timestamp, &[]).unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message_type, MessageType::User);
        assert_eq!(messages[0].message.role, "user");
    }

    #[test]
    fn test_empty_tool_result_content_fixed() {
        let complex_data = json!({
            "id": "msg-123",
            "role": "assistant",
            "content": [
                {
                    "type": "tool-result",
                    "toolCallId": "call-empty",
                    "output": "",
                    "is_error": false
                }
            ]
        });

        let blob = CursorBlob {
            content_wrapper: None,
            uuid: Some("test-uuid".to_string()),
            metadata: Some(String::new()),
            complex_data: Some(complex_data.to_string()),
            additional_content: None,
            blob_references: None,
        };

        let timestamp = "2025-01-01T00:00:00.000Z";
        let messages = split_complex_message(&blob, timestamp, &[]).unwrap();

        assert_eq!(messages.len(), 1);
        match &messages[0].message.content {
            ContentValue::Structured(blocks) => {
                match &blocks[0] {
                    ContentBlock::ToolResult { content, .. } => {
                        assert_eq!(content, "(no output)", "Empty content should be replaced");
                    }
                    _ => panic!("Expected tool_result block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_tool_use_id_matches_tool_result() {
        let tool_id = "call-test-123";
        let complex_data = json!({
            "id": "msg-123",
            "role": "assistant",
            "content": [
                {
                    "type": "tool-call",
                    "toolCallId": tool_id,
                    "toolName": "read_file",
                    "args": {"path": "/test/file.txt"}
                },
                {
                    "type": "tool-result",
                    "toolCallId": tool_id,
                    "output": "file contents",
                    "is_error": false
                }
            ]
        });

        let blob = CursorBlob {
            content_wrapper: None,
            uuid: Some("test-uuid".to_string()),
            metadata: Some(String::new()),
            complex_data: Some(complex_data.to_string()),
            additional_content: None,
            blob_references: None,
        };

        let timestamp = "2025-01-01T00:00:00.000Z";
        let messages = split_complex_message(&blob, timestamp, &[]).unwrap();

        // Extract tool_use id
        let tool_use_id = match &messages[0].message.content {
            ContentValue::Structured(blocks) => match &blocks[0] {
                ContentBlock::ToolUse { id, .. } => id.clone(),
                _ => panic!("Expected tool_use block"),
            },
            _ => panic!("Expected structured content"),
        };

        // Extract tool_result tool_use_id
        let tool_result_id = match &messages[1].message.content {
            ContentValue::Structured(blocks) => match &blocks[0] {
                ContentBlock::ToolResult { tool_use_id, .. } => tool_use_id.clone(),
                _ => panic!("Expected tool_result block"),
            },
            _ => panic!("Expected structured content"),
        };

        assert_eq!(tool_use_id, tool_result_id, "Tool use ID must match tool result tool_use_id");
        assert_eq!(tool_use_id, tool_id, "IDs must match original tool_id");
    }

    #[test]
    fn test_thinking_blocks_separate_message() {
        let complex_data = json!({
            "id": "msg-123",
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "Let me think about this."
                },
                {
                    "type": "redacted-reasoning",
                    "data": "thinking data here"
                }
            ]
        });

        let blob = CursorBlob {
            content_wrapper: None,
            uuid: Some("test-uuid".to_string()),
            metadata: Some(String::new()),
            complex_data: Some(complex_data.to_string()),
            additional_content: None,
            blob_references: None,
        };

        let timestamp = "2025-01-01T00:00:00.000Z";
        let messages = split_complex_message(&blob, timestamp, &[]).unwrap();

        // Should create 2 messages: text + thinking
        assert_eq!(messages.len(), 2);

        // First message should be text
        match &messages[0].message.content {
            ContentValue::Text(text) => assert_eq!(text, "Let me think about this."),
            _ => panic!("Expected text content"),
        }

        // Second message should be thinking block
        match &messages[1].message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Thinking { thinking } => {
                        assert!(thinking.contains("Redacted reasoning"));
                    }
                    _ => panic!("Expected thinking block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_multiple_text_blocks_combined() {
        let complex_data = json!({
            "id": "msg-123",
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "First part."
                },
                {
                    "type": "text",
                    "text": "Second part."
                }
            ]
        });

        let blob = CursorBlob {
            content_wrapper: None,
            uuid: Some("test-uuid".to_string()),
            metadata: Some(String::new()),
            complex_data: Some(complex_data.to_string()),
            additional_content: None,
            blob_references: None,
        };

        let timestamp = "2025-01-01T00:00:00.000Z";
        let messages = split_complex_message(&blob, timestamp, &[]).unwrap();

        // Should combine into one text message
        assert_eq!(messages.len(), 1);
        match &messages[0].message.content {
            ContentValue::Text(text) => assert_eq!(text, "First part.\nSecond part."),
            _ => panic!("Expected text content"),
        }
    }
}
