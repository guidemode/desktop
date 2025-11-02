use crate::providers::canonical::{
    CanonicalMessage, ContentBlock, ContentValue, MessageContent, MessageType,
};
use crate::providers::copilot_parser::CopilotEvent;
use anyhow::{Context, Result};
use serde_json::Value;
use uuid::Uuid;

/// Convert a Copilot event to one or more canonical messages
///
/// Maps new event types to canonical format:
/// - session.start → meta message
/// - user.message → user message
/// - assistant.message → assistant message with optional tool requests
/// - tool.execution_start → tool_use
/// - tool.execution_complete → tool_result
pub fn convert_event_to_canonical(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<Vec<CanonicalMessage>> {
    match event.event_type.as_str() {
        "session.start" => Ok(vec![convert_session_start(event, session_id, cwd)?]),
        "user.message" => Ok(vec![convert_user_message(event, session_id, cwd)?]),
        "assistant.message" => Ok(vec![convert_assistant_message(event, session_id, cwd)?]),
        "tool.execution_start" => Ok(vec![convert_tool_use(event, session_id, cwd)?]),
        "tool.execution_complete" => Ok(vec![convert_tool_result(event, session_id, cwd)?]),
        "session.info" => Ok(vec![convert_info_message(event, session_id, cwd)?]),
        _ => {
            // Unknown type - create a meta message
            Ok(vec![convert_unknown_message(event, session_id, cwd)?])
        }
    }
}

/// Convert session.start event
fn convert_session_start(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<CanonicalMessage> {
    Ok(CanonicalMessage {
        uuid: event.id.clone(),
        timestamp: event.timestamp.clone(),
        message_type: MessageType::Meta,
        session_id: session_id.to_string(),
        provider: "github-copilot".to_string(),
        cwd: cwd.map(String::from),
        git_branch: None,
        version: None,
        parent_uuid: event.parent_id.clone(),
        is_sidechain: None,
        user_type: None,
        message: MessageContent {
            role: "meta".to_string(),
            content: ContentValue::Text("Session started".to_string()),
            model: None,
            usage: None,
        },
        provider_metadata: Some(event.data.clone()),
        is_meta: Some(true),
        request_id: None,
        tool_use_result: None,
    })
}

/// Convert user.message event
fn convert_user_message(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<CanonicalMessage> {
    let text = event
        .data
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(CanonicalMessage {
        uuid: event.id.clone(),
        timestamp: event.timestamp.clone(),
        message_type: MessageType::User,
        session_id: session_id.to_string(),
        provider: "github-copilot".to_string(),
        cwd: cwd.map(String::from),
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: Some("external".to_string()),
        message: MessageContent {
            role: "user".to_string(),
            content: ContentValue::Text(text),
            model: None,
            usage: None,
        },
        provider_metadata: Some(serde_json::json!({
            "copilot_type": "user",
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Convert assistant.message event
fn convert_assistant_message(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<CanonicalMessage> {
    let id = extract_id(event);
    let timestamp = extract_timestamp(event)?;
    let text = extract_text(event);

    // Check for intention summary
    let intention = event
        .data
        .get("intentionSummary")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(CanonicalMessage {
        uuid: id,
        timestamp,
        message_type: MessageType::Assistant,
        session_id: session_id.to_string(),
        provider: "github-copilot".to_string(),
        cwd: cwd.map(String::from),
        git_branch: None,
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
            "copilot_type": "copilot",
            "has_intention": intention.is_some(),
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Convert info message timeline entry
fn convert_info_message(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<CanonicalMessage> {
    let id = extract_id(event);
    let timestamp = extract_timestamp(event)?;
    let text = extract_text(event);

    Ok(CanonicalMessage {
        uuid: id,
        timestamp,
        message_type: MessageType::Meta,
        session_id: session_id.to_string(),
        provider: "github-copilot".to_string(),
        cwd: cwd.map(String::from),
        git_branch: None,
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
            "copilot_type": "info",
        })),
        is_meta: Some(true),
        request_id: None,
        tool_use_result: None,
    })
}

/// Convert tool_call_requested or tool_call_completed to tool_use message
fn convert_tool_use(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<CanonicalMessage> {
    let id = extract_id(event);
    let timestamp = extract_timestamp(event)?;

    // Extract tool call details
    let call_id = event
        .data
        .get("callId")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();

    let tool_name = event
        .data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Parse arguments (can be a JSON string or object)
    let arguments = if let Some(args) = event.data.get("arguments") {
        if let Some(args_str) = args.as_str() {
            // Try to parse JSON string
            serde_json::from_str(args_str).unwrap_or(Value::String(args_str.to_string()))
        } else {
            // Already an object
            args.clone()
        }
    } else {
        Value::Null
    };

    let tool_block = ContentBlock::ToolUse {
        id: call_id.clone(),
        name: tool_name,
        input: arguments,
    };

    // Check for intention summary and tool title
    let intention = event
        .data
        .get("intentionSummary")
        .and_then(|v| v.as_str())
        .map(String::from);
    let tool_title = event
        .data
        .get("toolTitle")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(CanonicalMessage {
        uuid: call_id,
        timestamp,
        message_type: MessageType::Assistant,
        session_id: session_id.to_string(),
        provider: "github-copilot".to_string(),
        cwd: cwd.map(String::from),
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: Some("external".to_string()),
        message: MessageContent {
            role: "assistant".to_string(),
            content: ContentValue::Structured(vec![tool_block]),
            model: None,
            usage: None,
        },
        provider_metadata: Some(serde_json::json!({
            "copilot_type": "tool_call_requested",
            "has_intention": intention.is_some(),
            "has_tool_title": tool_title.is_some(),
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Convert tool_call_completed to tool_result message
fn convert_tool_result(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<CanonicalMessage> {
    let id = extract_id(event);
    let timestamp = extract_timestamp(event)?;

    // Extract call ID
    let call_id = event
        .data
        .get("callId")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();

    // Extract result
    let result_content = if let Some(result) = event.data.get("result") {
        if result.is_string() {
            result.as_str().unwrap_or("").to_string()
        } else {
            serde_json::to_string(result).unwrap_or_default()
        }
    } else {
        String::new()
    };

    let tool_result_block = ContentBlock::ToolResult {
        tool_use_id: call_id.clone(),
        content: result_content,
        is_error: Some(false), // Copilot doesn't clearly indicate errors
    };

    Ok(CanonicalMessage {
        uuid: format!("{}_result", id),
        timestamp,
        message_type: MessageType::Assistant,
        session_id: session_id.to_string(),
        provider: "github-copilot".to_string(),
        cwd: cwd.map(String::from),
        git_branch: None,
        version: None,
        parent_uuid: Some(call_id),
        is_sidechain: None,
        user_type: Some("external".to_string()),
        message: MessageContent {
            role: "assistant".to_string(),
            content: ContentValue::Structured(vec![tool_result_block]),
            model: None,
            usage: None,
        },
        provider_metadata: Some(serde_json::json!({
            "copilot_type": "tool_result",
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Convert unknown message type to meta message
fn convert_unknown_message(
    event: &CopilotEvent,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<CanonicalMessage> {
    let id = extract_id(event);
    let timestamp = extract_timestamp(event)?;
    let text = extract_text(event);
    let event_type = &event.event_type;

    Ok(CanonicalMessage {
        uuid: id,
        timestamp,
        message_type: MessageType::Meta,
        session_id: session_id.to_string(),
        provider: "github-copilot".to_string(),
        cwd: cwd.map(String::from),
        git_branch: None,
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
            "copilot_type": event_type,
            "warning": "unknown_type",
        })),
        is_meta: Some(true),
        request_id: None,
        tool_use_result: None,
    })
}

/// Extract ID from timeline entry
fn extract_id(event: &CopilotEvent) -> String {
    event.id.clone()
}

/// Extract timestamp from event
fn extract_timestamp(event: &CopilotEvent) -> Result<String> {
    Ok(event.timestamp.clone())
}

/// Extract text content from event data
fn extract_text(event: &CopilotEvent) -> String {
    event
        .data
        .get("content")
        .or_else(|| event.data.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_event(event_type: &str, data: Value) -> CopilotEvent {
        CopilotEvent {
            event_type: event_type.to_string(),
            data,
            id: "test-123".to_string(),
            timestamp: "2025-01-01T10:00:00.000Z".to_string(),
            parent_id: None,
        }
    }

    #[test]
    fn test_convert_user_message() {
        let entry = create_test_event("user.message", json!({ "content": "Hello", "attachments": [] }));
        let result = convert_event_to_canonical(&entry, "session-1", Some("/test"))
            .unwrap();

        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.message_type, MessageType::User);
        assert_eq!(msg.session_id, "session-1");
        assert_eq!(msg.cwd, Some("/test".to_string()));

        match &msg.message.content {
            ContentValue::Text(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_assistant_message() {
        let entry = create_test_event("assistant.message", json!({ "content": "Hi there", "messageId": "msg-1", "toolRequests": [] }));
        let result = convert_event_to_canonical(&entry, "session-1", None).unwrap();

        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.message_type, MessageType::Assistant);

        match &msg.message.content {
            ContentValue::Text(text) => assert_eq!(text, "Hi there"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_tool_execution_start() {
        let entry = create_test_event(
            "tool.execution_start",
            json!({
                "toolCallId": "call-456",
                "toolName": "bash",
                "arguments": {"command": "ls -la", "path": "/test"}
            }),
        );
        let result = convert_event_to_canonical(&entry, "session-1", None).unwrap();

        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.message_type, MessageType::Assistant);
        assert_eq!(msg.uuid, "call-456");

        match &msg.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call-456");
                        assert_eq!(name, "read_file");
                    }
                    _ => panic!("Expected tool_use block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_convert_tool_execution_complete() {
        let entry = create_test_event(
            "tool.execution_complete",
            json!({
                "toolCallId": "call-789",
                "success": true,
                "result": {"content": "File contents here"}
            }),
        );
        let result = convert_event_to_canonical(&entry, "session-1", None).unwrap();

        // Should produce 1 message: tool_result
        assert_eq!(result.len(), 1);

        // First message: tool_use
        let tool_use = &result[0];
        assert_eq!(tool_use.message_type, MessageType::Assistant);
        assert_eq!(tool_use.uuid, "call-789");
        match &tool_use.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse { id, .. } => assert_eq!(id, "call-789"),
                    _ => panic!("Expected tool_use block"),
                }
            }
            _ => panic!("Expected structured content"),
        }

        // Second message: tool_result
        let tool_result = &result[1];
        assert_eq!(tool_result.message_type, MessageType::Assistant);
        assert_eq!(tool_result.parent_uuid, Some("call-789".to_string()));
        match &tool_result.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult { tool_use_id, content, .. } => {
                        assert_eq!(tool_use_id, "call-789");
                        assert_eq!(content, "File contents here");
                    }
                    _ => panic!("Expected tool_result block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_convert_info_message() {
        let entry = create_test_event("session.info", json!({ "infoType": "mcp", "message": "Connected to MCP" }));
        let result = convert_event_to_canonical(&entry, "session-1", None).unwrap();

        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.message_type, MessageType::Meta);
        assert_eq!(msg.is_meta, Some(true));
    }

    #[test]
    fn test_convert_unknown_type() {
        let entry = create_test_event("weird_type", json!({ "text": "Unknown" }));
        let result = convert_event_to_canonical(&entry, "session-1", None).unwrap();

        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.message_type, MessageType::Meta);

        let metadata = msg.provider_metadata.as_ref().unwrap();
        assert_eq!(metadata["copilot_type"], "weird_type");
        assert_eq!(metadata["warning"], "unknown_type");
    }
}
