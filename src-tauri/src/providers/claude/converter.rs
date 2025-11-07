//! Converter implementation for Claude Code to canonical format

use crate::providers::canonical::{
    converter::ToCanonical, CanonicalMessage, MessageContent, MessageType,
};
use anyhow::{Context, Result};

use super::types::{ClaudeEntry, ClaudeEntryType};

/// Claude Code converter
///
/// Converts native Claude Code JSONL format to canonical format.
/// The main transformation is:
/// 1. Filter out system events (file-history-snapshot, summary, etc.)
/// 2. Add `provider: "claude-code"` field
/// 3. Clean up nullable fields
#[allow(dead_code)] // Kept for documentation, conversion done via ToCanonical trait
pub struct ClaudeConverter;

impl ToCanonical for ClaudeEntry {
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
        // 1. Filter out non-conversational events
        if self.should_filter() {
            return Ok(None);
        }

        // 2. Only process conversational types
        if !self.is_conversational() {
            return Ok(None);
        }

        // 3. Extract required fields
        let uuid = self
            .uuid
            .as_ref()
            .context("Missing uuid in Claude entry")?
            .clone();

        let timestamp = self
            .timestamp
            .as_ref()
            .context("Missing timestamp in Claude entry")?
            .clone();

        let session_id = self
            .session_id
            .as_ref()
            .context("Missing sessionId in Claude entry")?
            .clone();

        // 4. Map entry type to message type
        let message_type = match &self.entry_type {
            ClaudeEntryType::User => MessageType::User,
            ClaudeEntryType::Assistant => MessageType::Assistant,
            ClaudeEntryType::Meta => MessageType::Meta,
            ClaudeEntryType::System => MessageType::Meta,
            _ => {
                // This shouldn't happen due to filtering, but handle it gracefully
                return Ok(None);
            }
        };

        // 5. Extract and clean message content
        let message: MessageContent = if let Some(msg_value) = &self.message {
            let mut msg: MessageContent = serde_json::from_value(msg_value.clone())
                .context("Failed to parse message content")?;

            // Fix empty tool_result content (canonical schema requires non-empty content)
            if let crate::providers::canonical::ContentValue::Structured(ref mut blocks) = msg.content {
                for block in blocks.iter_mut() {
                    if let crate::providers::canonical::ContentBlock::ToolResult { content, .. } = block {
                        if content.is_empty() {
                            *content = "(no output)".to_string();
                        }
                    }
                }
            }

            msg
        } else {
            // If no message content, create empty meta message
            MessageContent {
                role: "assistant".to_string(),
                content: crate::providers::canonical::ContentValue::Text(String::new()),
                model: None,
                usage: None,
            }
        };

        // 6. Clean up parent_uuid (remove null values)
        let parent_uuid = self
            .parent_uuid
            .clone()
            .filter(|s| !s.is_empty() && s != "null");

        // 7. Build canonical message
        Ok(Some(CanonicalMessage {
            uuid,
            timestamp,
            message_type,
            session_id,
            provider: "claude-code".to_string(), // ← ADD PROVIDER FIELD
            cwd: self.cwd.clone(),
            git_branch: self.git_branch.clone(),
            version: self.version.clone(),
            parent_uuid,
            is_sidechain: self.is_sidechain,
            user_type: self.user_type.clone(),
            message,
            provider_metadata: None, // Claude IS canonical, no metadata needed
            is_meta: self.is_meta,
            request_id: self.request_id.clone(),
            tool_use_result: self.tool_use_result.clone(),
        }))
    }

    fn provider_name(&self) -> &str {
        "claude-code"
    }

    fn extract_cwd(&self) -> Option<String> {
        self.cwd.clone()
    }

    fn extract_git_branch(&self) -> Option<String> {
        self.git_branch.clone()
    }

    fn extract_version(&self) -> Option<String> {
        self.version.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::canonical::ContentValue;

    #[test]
    fn test_convert_user_message() {
        let json = r#"{
            "parentUuid": null,
            "isSidechain": true,
            "userType": "external",
            "cwd": "/Users/test/project",
            "sessionId": "abc-123",
            "version": "2.0.21",
            "gitBranch": "main",
            "type": "user",
            "message": {"role": "user", "content": "Hello"},
            "uuid": "uuid-1",
            "timestamp": "2025-10-20T07:44:31.563Z"
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        let canonical = entry.to_canonical().unwrap().unwrap();

        assert_eq!(canonical.uuid, "uuid-1");
        assert_eq!(canonical.provider, "claude-code");
        assert_eq!(canonical.session_id, "abc-123");
        assert_eq!(canonical.cwd, Some("/Users/test/project".to_string()));
        assert_eq!(canonical.git_branch, Some("main".to_string()));
        assert_eq!(canonical.version, Some("2.0.21".to_string()));
        assert_eq!(canonical.message_type, MessageType::User);
        assert_eq!(canonical.parent_uuid, None); // null should be filtered out

        match canonical.message.content {
            ContentValue::Text(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_assistant_message() {
        let json = r#"{
            "parentUuid": "uuid-1",
            "isSidechain": true,
            "userType": "external",
            "cwd": "/Users/test/project",
            "sessionId": "abc-123",
            "version": "2.0.21",
            "gitBranch": "main",
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hi there!"}],
                "model": "claude-sonnet-4-5-20250929"
            },
            "uuid": "uuid-2",
            "timestamp": "2025-10-20T07:44:35.123Z",
            "requestId": "req_123"
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        let canonical = entry.to_canonical().unwrap().unwrap();

        assert_eq!(canonical.uuid, "uuid-2");
        assert_eq!(canonical.provider, "claude-code");
        assert_eq!(canonical.message_type, MessageType::Assistant);
        assert_eq!(canonical.parent_uuid, Some("uuid-1".to_string()));
        assert_eq!(canonical.request_id, Some("req_123".to_string()));
        assert_eq!(canonical.message.model, Some("claude-sonnet-4-5-20250929".to_string()));
    }

    #[test]
    fn test_filter_file_history_snapshot() {
        let json = r#"{
            "type": "file-history-snapshot",
            "messageId": "msg-123",
            "snapshot": {
                "messageId": "msg-123",
                "trackedFileBackups": {},
                "timestamp": "2025-10-23T03:50:05.757Z"
            },
            "isSnapshotUpdate": false
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        let result = entry.to_canonical().unwrap();

        assert!(result.is_none(), "file-history-snapshot should be filtered out");
    }

    #[test]
    fn test_filter_summary() {
        let json = r#"{
            "type": "summary",
            "uuid": "summary-123",
            "timestamp": "2025-10-20T08:00:00.000Z",
            "sessionId": "abc-123",
            "message": {"role": "assistant", "content": "Session summary"}
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        let result = entry.to_canonical().unwrap();

        assert!(result.is_none(), "summary should be filtered out");
    }

    #[test]
    fn test_filter_system_compact_boundary() {
        let json = r#"{
            "type": "system",
            "subtype": "compact_boundary",
            "content": "Conversation compacted",
            "timestamp": "2025-10-20T08:00:00.000Z"
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        let result = entry.to_canonical().unwrap();

        assert!(result.is_none(), "system compact_boundary should be filtered out");
    }

    #[test]
    fn test_convert_with_structured_content() {
        let json = r#"{
            "type": "assistant",
            "uuid": "uuid-3",
            "timestamp": "2025-10-20T07:45:00.000Z",
            "sessionId": "abc-123",
            "cwd": "/test",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "Let me run a command"},
                    {"type": "tool_use", "id": "tool-1", "name": "bash", "input": {"command": "ls"}}
                ],
                "model": "claude-sonnet-4-5-20250929"
            }
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        let canonical = entry.to_canonical().unwrap().unwrap();

        assert_eq!(canonical.provider, "claude-code");
        assert_eq!(canonical.message_type, MessageType::Assistant);

        match canonical.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 2);
            }
            _ => panic!("Expected structured content"),
        }
    }
}
