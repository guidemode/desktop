use crate::providers::canonical::{
    converter::ToCanonical, CanonicalMessage, ContentBlock, ContentValue, MessageContent,
    MessageType, TokenUsage,
};
use crate::providers::gemini_parser::{GeminiMessage, GeminiSession};
use anyhow::{Context, Result};
use serde_json::Value;

/// Implement ToCanonical for GeminiMessage
///
/// Gemini messages need special handling:
/// - Type "gemini" maps to "assistant"
/// - Tool calls are split into separate tool_use/tool_result messages
/// - Thoughts and token usage preserved in metadata
impl ToCanonical for GeminiMessage {
    fn to_canonical(&self) -> Result<CanonicalMessage> {
        // Map "gemini" type to "assistant"
        let message_type = match self.message_type.as_str() {
            "user" => MessageType::User,
            "gemini" | "assistant" => MessageType::Assistant,
            _ => MessageType::Meta,
        };

        let role = if message_type == MessageType::User {
            "user"
        } else {
            "assistant"
        };

        // Build content value
        let content = if !self.content.is_empty() {
            ContentValue::Text(self.content.clone())
        } else {
            ContentValue::Text(String::new())
        };

        // Map Gemini tokens to canonical usage
        let usage = self.tokens.as_ref().map(|tokens| TokenUsage {
            input_tokens: Some(tokens.input),
            output_tokens: Some(tokens.output),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: Some(tokens.cached),
        });

        // Preserve only minimal metadata (DON'T duplicate full data!)
        let provider_metadata = serde_json::json!({
            "gemini_type": self.message_type,
            "has_thoughts": self.thoughts.is_some(),
            "has_tool_calls": self.tool_calls.is_some(),
        });

        Ok(CanonicalMessage {
            uuid: self.id.clone(),
            timestamp: self.timestamp.clone(),
            message_type,
            session_id: String::new(), // Will be filled by session converter
            provider: "gemini-code".to_string(),
            cwd: None, // Will be filled by session converter
            git_branch: None,
            version: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: Some("external".to_string()),
            message: MessageContent {
                role: role.to_string(),
                content,
                model: self.model.clone(),
                usage,
            },
            provider_metadata: Some(provider_metadata),
            is_meta: None,
            request_id: None,
            tool_use_result: None,
        })
    }

    fn provider_name(&self) -> &str {
        "gemini-code"
    }

    fn extract_cwd(&self) -> Option<String> {
        None // CWD is inferred at session level
    }
}

/// Convert a GeminiSession to canonical JSONL
///
/// This handles:
/// - Converting each message to canonical format
/// - Extracting tool calls into separate tool_use/tool_result messages
/// - Populating session_id and cwd for all messages
pub fn convert_session_to_canonical(
    session: &GeminiSession,
    cwd: Option<String>,
) -> Result<Vec<CanonicalMessage>> {
    let mut canonical_messages = Vec::new();

    for message in &session.messages {
        // First, handle tool calls if present
        if let Some(ref tool_calls) = message.tool_calls {
            for tool_call in tool_calls {
                // Create tool_use message
                let tool_use_block = ContentBlock::ToolUse {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    input: tool_call.args.clone().unwrap_or(Value::Null),
                };

                let tool_use_msg = CanonicalMessage {
                    uuid: tool_call.id.clone(),
                    timestamp: message.timestamp.clone(),
                    message_type: MessageType::Assistant,
                    session_id: session.session_id.clone(),
                    provider: "gemini-code".to_string(),
                    cwd: cwd.clone(),
                    git_branch: None,
                    version: None,
                    parent_uuid: None,
                    is_sidechain: None,
                    user_type: Some("external".to_string()),
                    message: MessageContent {
                        role: "assistant".to_string(),
                        content: ContentValue::Structured(vec![tool_use_block]),
                        model: message.model.clone(),
                        usage: None,
                    },
                    provider_metadata: Some(serde_json::json!({
                        "gemini_type": "tool_call",
                        "tool_status": tool_call.status.clone(),
                    })),
                    is_meta: None,
                    request_id: None,
                    tool_use_result: None,
                };
                canonical_messages.push(tool_use_msg);

                // Create tool_result message if result exists
                if let Some(ref result) = tool_call.result {
                    // Extract the actual output from Gemini's functionResponse wrapper
                    // Result format: [{ functionResponse: { response: { output: "..." } } }]
                    let content_str = if let Some(fr) =
                        result.first().and_then(|r| r.get("functionResponse"))
                    {
                        // Try to extract the response.output field for shell commands
                        if let Some(response) = fr.get("response") {
                            if let Some(output) = response.get("output") {
                                output.to_string()
                            } else {
                                // Fallback: serialize the whole response object
                                serde_json::to_string(response)?
                            }
                        } else {
                            // Fallback: serialize the whole functionResponse
                            serde_json::to_string(fr)?
                        }
                    } else {
                        // Fallback: serialize the raw result
                        serde_json::to_string(result)?
                    };

                    let tool_result_block = ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: content_str,
                        is_error: Some(tool_call.status.as_deref() != Some("success")),
                    };

                    let tool_result_msg = CanonicalMessage {
                        uuid: format!("{}_result", tool_call.id),
                        timestamp: message.timestamp.clone(),
                        message_type: MessageType::Assistant,
                        session_id: session.session_id.clone(),
                        provider: "gemini-code".to_string(),
                        cwd: cwd.clone(),
                        git_branch: None,
                        version: None,
                        parent_uuid: Some(tool_call.id.clone()),
                        is_sidechain: None,
                        user_type: Some("external".to_string()),
                        message: MessageContent {
                            role: "assistant".to_string(),
                            content: ContentValue::Structured(vec![tool_result_block]),
                            model: message.model.clone(),
                            usage: None,
                        },
                        provider_metadata: Some(serde_json::json!({
                            "gemini_type": "tool_result",
                        })),
                        is_meta: None,
                        request_id: None,
                        tool_use_result: None,
                    };
                    canonical_messages.push(tool_result_msg);
                }
            }
        }

        // Then, emit main message if it has content or thoughts
        if !message.content.is_empty() || message.thoughts.is_some() {
            let mut canonical_msg = message.to_canonical()?;

            // Fill in session-level fields
            canonical_msg.session_id = session.session_id.clone();
            canonical_msg.cwd = cwd.clone();

            // Add thoughts to metadata if present
            if let Some(ref thoughts) = message.thoughts {
                if let Some(ref mut metadata) = canonical_msg.provider_metadata {
                    if let Some(obj) = metadata.as_object_mut() {
                        obj.insert(
                            "gemini_thoughts".to_string(),
                            serde_json::to_value(thoughts)
                                .context("Failed to serialize thoughts")?,
                        );
                    }
                }
            }

            canonical_messages.push(canonical_msg);
        }
    }

    Ok(canonical_messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::gemini_parser::{Thought, TokenUsage as GeminiTokenUsage, ToolCall};

    #[test]
    fn test_convert_user_message() {
        let msg = GeminiMessage {
            id: "msg-1".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            message_type: "user".to_string(),
            content: "Hello".to_string(),
            tool_calls: None,
            thoughts: None,
            tokens: None,
            model: None,
        };

        let canonical = msg.to_canonical().unwrap();

        assert_eq!(canonical.uuid, "msg-1");
        assert_eq!(canonical.message_type, MessageType::User);
        assert_eq!(canonical.message.role, "user");

        match canonical.message.content {
            ContentValue::Text(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_assistant_message_with_tokens() {
        let msg = GeminiMessage {
            id: "msg-2".to_string(),
            timestamp: "2025-01-01T00:00:01.000Z".to_string(),
            message_type: "gemini".to_string(),
            content: "World".to_string(),
            tool_calls: None,
            thoughts: None,
            tokens: Some(GeminiTokenUsage {
                input: 100,
                output: 50,
                cached: 75,
                thoughts: 10,
                tool: 0,
                total: 160,
            }),
            model: Some("gemini-2.0-flash-exp".to_string()),
        };

        let canonical = msg.to_canonical().unwrap();

        assert_eq!(canonical.message_type, MessageType::Assistant);
        assert_eq!(canonical.message.role, "assistant");
        assert_eq!(canonical.message.model, Some("gemini-2.0-flash-exp".to_string()));

        let usage = canonical.message.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.cache_read_input_tokens, Some(75));
    }

    #[test]
    fn test_convert_session_with_tool_calls() {
        let session = GeminiSession {
            session_id: "session-123".to_string(),
            project_hash: "abc123".to_string(),
            start_time: "2025-01-01T00:00:00.000Z".to_string(),
            last_updated: "2025-01-01T00:00:10.000Z".to_string(),
            messages: vec![GeminiMessage {
                id: "msg-1".to_string(),
                timestamp: "2025-01-01T00:00:05.000Z".to_string(),
                message_type: "gemini".to_string(),
                content: String::new(),
                tool_calls: Some(vec![ToolCall {
                    id: "call-1".to_string(),
                    name: "shell".to_string(),
                    args: Some(serde_json::json!({"command": "ls"})),
                    result: Some(vec![serde_json::json!({
                        "functionResponse": {
                            "response": {
                                "output": "file1.txt\nfile2.txt"
                            }
                        }
                    })]),
                    status: Some("success".to_string()),
                    extra: std::collections::HashMap::new(),
                }]),
                thoughts: None,
                tokens: None,
                model: None,
            }],
        };

        let canonical = convert_session_to_canonical(&session, Some("/test/path".to_string()))
            .unwrap();

        // Should have 2 messages: tool_use + tool_result
        assert_eq!(canonical.len(), 2);

        // First message: tool_use
        assert_eq!(canonical[0].uuid, "call-1");
        assert_eq!(canonical[0].session_id, "session-123");
        assert_eq!(canonical[0].cwd, Some("/test/path".to_string()));

        match &canonical[0].message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call-1");
                        assert_eq!(name, "shell");
                    }
                    _ => panic!("Expected ToolUse block"),
                }
            }
            _ => panic!("Expected structured content"),
        }

        // Second message: tool_result
        assert_eq!(canonical[1].uuid, "call-1_result");
        assert_eq!(canonical[1].parent_uuid, Some("call-1".to_string()));

        match &canonical[1].message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } => {
                        assert_eq!(tool_use_id, "call-1");
                        assert!(content.contains("file1.txt"));
                    }
                    _ => panic!("Expected ToolResult block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_minimal_metadata() {
        let msg = GeminiMessage {
            id: "msg-1".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            message_type: "user".to_string(),
            content: "Test".to_string(),
            tool_calls: None,
            thoughts: Some(vec![Thought {
                subject: "test".to_string(),
                description: "testing".to_string(),
                timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            }]),
            tokens: None,
            model: None,
        };

        let canonical = msg.to_canonical().unwrap();
        let metadata = canonical.provider_metadata.unwrap();

        // Should only have minimal flags, not full data
        assert_eq!(metadata["gemini_type"], "user");
        assert_eq!(metadata["has_thoughts"], true);
        assert!(metadata.get("thoughts").is_none()); // Full thoughts not in metadata
    }
}
