use super::*;
use serde_json::json;

#[test]
fn test_deserialize_simple_text_message() {
    let json = r#"{
        "uuid": "test-uuid-123",
        "timestamp": "2025-01-01T00:00:00.000Z",
        "type": "user",
        "sessionId": "session-abc",
        "provider": "claude-code",
        "cwd": "/path/to/project",
        "message": {
            "role": "user",
            "content": "Hello, world!"
        }
    }"#;

    let msg: CanonicalMessage = serde_json::from_str(json).unwrap();

    assert_eq!(msg.uuid, "test-uuid-123");
    assert_eq!(msg.timestamp, "2025-01-01T00:00:00.000Z");
    assert_eq!(msg.message_type, MessageType::User);
    assert_eq!(msg.session_id, "session-abc");
    assert_eq!(msg.provider, "claude-code");
    assert_eq!(msg.cwd, Some("/path/to/project".to_string()));
    assert_eq!(msg.message.role, "user");

    match msg.message.content {
        ContentValue::Text(text) => assert_eq!(text, "Hello, world!"),
        _ => panic!("Expected text content"),
    }
}

#[test]
fn test_deserialize_structured_message_with_tool_use() {
    let json = r#"{
        "uuid": "msg-456",
        "timestamp": "2025-01-01T00:01:00.000Z",
        "type": "assistant",
        "sessionId": "session-abc",
        "provider": "claude-code",
        "message": {
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "Let me read that file for you."
                },
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "Read",
                    "input": {
                        "file_path": "/test/file.txt"
                    }
                }
            ],
            "model": "claude-sonnet-4-5-20250929"
        }
    }"#;

    let msg: CanonicalMessage = serde_json::from_str(json).unwrap();

    assert_eq!(msg.message_type, MessageType::Assistant);
    assert_eq!(msg.message.model, Some("claude-sonnet-4-5-20250929".to_string()));

    match msg.message.content {
        ContentValue::Structured(blocks) => {
            assert_eq!(blocks.len(), 2);

            match &blocks[0] {
                ContentBlock::Text { text } => {
                    assert_eq!(text, "Let me read that file for you.");
                }
                _ => panic!("Expected text block"),
            }

            match &blocks[1] {
                ContentBlock::ToolUse { id, name, input } => {
                    assert_eq!(id, "toolu_123");
                    assert_eq!(name, "Read");
                    assert_eq!(input["file_path"], "/test/file.txt");
                }
                _ => panic!("Expected tool_use block"),
            }
        }
        _ => panic!("Expected structured content"),
    }
}

#[test]
fn test_deserialize_tool_result() {
    let json = r#"{
        "uuid": "msg-789",
        "timestamp": "2025-01-01T00:02:00.000Z",
        "type": "assistant",
        "sessionId": "session-abc",
        "provider": "claude-code",
        "message": {
            "role": "assistant",
            "content": [
                {
                    "type": "tool_result",
                    "tool_use_id": "toolu_123",
                    "content": "File contents here...",
                    "is_error": false
                }
            ]
        }
    }"#;

    let msg: CanonicalMessage = serde_json::from_str(json).unwrap();

    match msg.message.content {
        ContentValue::Structured(blocks) => {
            assert_eq!(blocks.len(), 1);

            match &blocks[0] {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    assert_eq!(tool_use_id, "toolu_123");
                    assert_eq!(content, "File contents here...");
                    assert_eq!(*is_error, Some(false));
                }
                _ => panic!("Expected tool_result block"),
            }
        }
        _ => panic!("Expected structured content"),
    }
}

#[test]
fn test_serialize_with_provider_metadata() {
    let msg = CanonicalMessage {
        uuid: "test-uuid".to_string(),
        timestamp: "2025-01-01T00:00:00.000Z".to_string(),
        message_type: MessageType::Assistant,
        session_id: "session-id".to_string(),
        provider: "gemini-code".to_string(),
        cwd: Some("/path".to_string()),
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: Some("external".to_string()),
        message: MessageContent {
            role: "assistant".to_string(),
            content: ContentValue::Text("Response text".to_string()),
            model: Some("gemini-2.5-pro".to_string()),
            usage: None,
        },
        provider_metadata: Some(json!({
            "provider": "gemini-code",
            "gemini_thoughts": [
                {
                    "subject": "Test thought",
                    "description": "Testing metadata preservation"
                }
            ],
            "gemini_tokens": {
                "input": 100,
                "output": 50,
                "thoughts": 25
            }
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    };

    let json = serde_json::to_string(&msg).unwrap();

    // Verify serialization includes provider metadata
    assert!(json.contains("providerMetadata"));
    assert!(json.contains("gemini_thoughts"));
    assert!(json.contains("gemini_tokens"));

    // Verify round-trip
    let deserialized: CanonicalMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.uuid, msg.uuid);
    assert_eq!(deserialized.provider, "gemini-code");
    assert!(deserialized.provider_metadata.is_some());

    let metadata = deserialized.provider_metadata.unwrap();
    assert_eq!(metadata["provider"], "gemini-code");
    assert!(metadata["gemini_thoughts"].is_array());
}

#[test]
fn test_serialize_with_token_usage() {
    let msg = CanonicalMessage {
        uuid: "test-uuid".to_string(),
        timestamp: "2025-01-01T00:00:00.000Z".to_string(),
        message_type: MessageType::Assistant,
        session_id: "session-id".to_string(),
        provider: "claude-code".to_string(),
        cwd: None,
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: Some("external".to_string()),
        message: MessageContent {
            role: "assistant".to_string(),
            content: ContentValue::Text("Response".to_string()),
            model: Some("claude-sonnet-4-5-20250929".to_string()),
            usage: Some(TokenUsage {
                input_tokens: Some(150),
                output_tokens: Some(75),
                cache_creation_input_tokens: Some(500),
                cache_read_input_tokens: Some(300),
            }),
        },
        provider_metadata: None,
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    };

    let json = serde_json::to_string_pretty(&msg).unwrap();

    assert!(json.contains("\"usage\""));
    assert!(json.contains("\"input_tokens\": 150"));
    assert!(json.contains("\"output_tokens\": 75"));
    assert!(json.contains("\"cache_creation_input_tokens\": 500"));
    assert!(json.contains("\"cache_read_input_tokens\": 300"));
}

#[test]
fn test_new_text_message_helper() {
    let msg = CanonicalMessage::new_text_message(
        "uuid-1".to_string(),
        "2025-01-01T00:00:00.000Z".to_string(),
        MessageType::User,
        "session-1".to_string(),
        "codex".to_string(),
        "user".to_string(),
        "Test message".to_string(),
    );

    assert_eq!(msg.uuid, "uuid-1");
    assert_eq!(msg.message_type, MessageType::User);
    assert_eq!(msg.provider, "codex");
    assert_eq!(msg.message.role, "user");

    match msg.message.content {
        ContentValue::Text(text) => assert_eq!(text, "Test message"),
        _ => panic!("Expected text content"),
    }
}

#[test]
fn test_new_structured_message_helper() {
    let blocks = vec![
        ContentBlock::Text {
            text: "Running command".to_string(),
        },
        ContentBlock::ToolUse {
            id: "tool-1".to_string(),
            name: "Bash".to_string(),
            input: json!({ "command": "ls -la" }),
        },
    ];

    let msg = CanonicalMessage::new_structured_message(
        "uuid-2".to_string(),
        "2025-01-01T00:00:00.000Z".to_string(),
        MessageType::Assistant,
        "session-1".to_string(),
        "claude-code".to_string(),
        "assistant".to_string(),
        blocks,
    );

    assert_eq!(msg.uuid, "uuid-2");
    assert_eq!(msg.message_type, MessageType::Assistant);

    match msg.message.content {
        ContentValue::Structured(blocks) => {
            assert_eq!(blocks.len(), 2);
        }
        _ => panic!("Expected structured content"),
    }
}

#[test]
fn test_optional_fields_omitted_when_none() {
    let msg = CanonicalMessage::new_text_message(
        "uuid-3".to_string(),
        "2025-01-01T00:00:00.000Z".to_string(),
        MessageType::User,
        "session-1".to_string(),
        "codex".to_string(),
        "user".to_string(),
        "Message".to_string(),
    );

    let json = serde_json::to_string(&msg).unwrap();

    // Optional fields should not be present when None
    assert!(!json.contains("\"cwd\""));
    assert!(!json.contains("\"gitBranch\""));
    assert!(!json.contains("\"version\""));
    assert!(!json.contains("\"parentUuid\""));
    assert!(!json.contains("\"providerMetadata\""));
    assert!(!json.contains("\"isMeta\""));
    assert!(!json.contains("\"model\""));
    assert!(!json.contains("\"usage\""));
}

#[test]
fn test_message_type_serialization() {
    assert_eq!(
        serde_json::to_string(&MessageType::User).unwrap(),
        "\"user\""
    );
    assert_eq!(
        serde_json::to_string(&MessageType::Assistant).unwrap(),
        "\"assistant\""
    );
    assert_eq!(
        serde_json::to_string(&MessageType::Meta).unwrap(),
        "\"meta\""
    );
}

#[test]
fn test_message_type_deserialization() {
    assert_eq!(
        serde_json::from_str::<MessageType>("\"user\"").unwrap(),
        MessageType::User
    );
    assert_eq!(
        serde_json::from_str::<MessageType>("\"assistant\"").unwrap(),
        MessageType::Assistant
    );
    assert_eq!(
        serde_json::from_str::<MessageType>("\"meta\"").unwrap(),
        MessageType::Meta
    );
}

#[test]
fn test_deserialize_thinking_block() {
    let json = r#"{
        "uuid": "msg-thinking",
        "timestamp": "2025-01-01T00:03:00.000Z",
        "type": "assistant",
        "sessionId": "session-abc",
        "provider": "gemini-code",
        "message": {
            "role": "assistant",
            "content": [
                {
                    "type": "thinking",
                    "thinking": "Analysis: Examining the code structure"
                },
                {
                    "type": "text",
                    "text": "Let me help you with that."
                }
            ],
            "model": "gemini-2.5-pro"
        }
    }"#;

    let msg: CanonicalMessage = serde_json::from_str(json).unwrap();

    assert_eq!(msg.message_type, MessageType::Assistant);
    assert_eq!(msg.provider, "gemini-code");

    match msg.message.content {
        ContentValue::Structured(blocks) => {
            assert_eq!(blocks.len(), 2);

            // First block should be thinking
            match &blocks[0] {
                ContentBlock::Thinking { thinking } => {
                    assert_eq!(thinking, "Analysis: Examining the code structure");
                }
                _ => panic!("Expected thinking block"),
            }

            // Second block should be text
            match &blocks[1] {
                ContentBlock::Text { text } => {
                    assert_eq!(text, "Let me help you with that.");
                }
                _ => panic!("Expected text block"),
            }
        }
        _ => panic!("Expected structured content"),
    }
}

#[test]
fn test_serialize_thinking_block() {
    let blocks = vec![
        ContentBlock::Thinking {
            thinking: "Planning: Determining the approach".to_string(),
        },
        ContentBlock::Text {
            text: "I'll proceed with the implementation.".to_string(),
        },
    ];

    let msg = CanonicalMessage::new_structured_message(
        "uuid-thinking".to_string(),
        "2025-01-01T00:00:00.000Z".to_string(),
        MessageType::Assistant,
        "session-1".to_string(),
        "gemini-code".to_string(),
        "assistant".to_string(),
        blocks,
    );

    let json = serde_json::to_string(&msg).unwrap();

    // Verify serialization includes thinking type
    assert!(json.contains("\"type\":\"thinking\""));
    assert!(json.contains("Planning: Determining the approach"));

    // Verify round-trip
    let deserialized: CanonicalMessage = serde_json::from_str(&json).unwrap();
    match deserialized.message.content {
        ContentValue::Structured(blocks) => {
            assert_eq!(blocks.len(), 2);
            match &blocks[0] {
                ContentBlock::Thinking { thinking } => {
                    assert_eq!(thinking, "Planning: Determining the approach");
                }
                _ => panic!("Expected thinking block"),
            }
        }
        _ => panic!("Expected structured content"),
    }
}
