//! Type definitions for native Claude Code JSONL format

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Claude JSONL entry type discriminator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClaudeEntryType {
    User,
    Assistant,
    Meta,
    System,
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot,
    Summary,
}

/// Native Claude Code JSONL entry
///
/// This represents the raw format from ~/.claude/projects/**/session.jsonl files.
/// Note: Claude Code logs are ALMOST canonical format, but they:
/// 1. Contain system events that need filtering (file-history-snapshot, summary)
/// 2. Lack the `provider` field
/// 3. Have nullable fields like `parentUuid: null`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeEntry {
    /// Unique message identifier
    pub uuid: Option<String>,

    /// ISO 8601 timestamp
    pub timestamp: Option<String>,

    /// Entry type
    #[serde(rename = "type")]
    pub entry_type: ClaudeEntryType,

    /// Session identifier
    pub session_id: Option<String>,

    /// Current working directory
    pub cwd: Option<String>,

    /// Git branch name
    pub git_branch: Option<String>,

    /// Claude Code version
    pub version: Option<String>,

    /// Parent message UUID (for threading)
    pub parent_uuid: Option<String>,

    /// Whether this is a sidechain message
    pub is_sidechain: Option<bool>,

    /// User type
    pub user_type: Option<String>,

    /// Message content
    pub message: Option<Value>,

    /// Request ID
    pub request_id: Option<String>,

    /// System event subtype (for system messages)
    pub subtype: Option<String>,

    /// Whether this is a meta message
    pub is_meta: Option<bool>,

    /// Content (for summary and other events)
    pub content: Option<String>,

    /// Message ID (for file-history-snapshot)
    pub message_id: Option<String>,

    /// Snapshot data (for file-history-snapshot)
    pub snapshot: Option<Value>,

    /// Tool use result
    pub tool_use_result: Option<Value>,
}

impl ClaudeEntry {
    /// Check if this entry should be filtered out
    pub fn should_filter(&self) -> bool {
        match &self.entry_type {
            ClaudeEntryType::FileHistorySnapshot => true,
            ClaudeEntryType::Summary => true,
            ClaudeEntryType::System => {
                // Filter out system events with specific subtypes
                if let Some(subtype) = &self.subtype {
                    matches!(
                        subtype.as_str(),
                        "compact_boundary" | "informational" | "compaction"
                    )
                } else {
                    // Keep system events without subtype (might be meta)
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if this is a conversational message type
    pub fn is_conversational(&self) -> bool {
        matches!(
            &self.entry_type,
            ClaudeEntryType::User | ClaudeEntryType::Assistant | ClaudeEntryType::Meta
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user_entry() {
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
        assert_eq!(entry.entry_type, ClaudeEntryType::User);
        assert_eq!(entry.uuid, Some("uuid-1".to_string()));
        assert_eq!(entry.session_id, Some("abc-123".to_string()));
        assert_eq!(entry.cwd, Some("/Users/test/project".to_string()));
        assert!(!entry.should_filter());
        assert!(entry.is_conversational());
    }

    #[test]
    fn test_parse_file_history_snapshot() {
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
        assert_eq!(entry.entry_type, ClaudeEntryType::FileHistorySnapshot);
        assert!(entry.should_filter());
        assert!(!entry.is_conversational());
    }

    #[test]
    fn test_parse_system_event() {
        let json = r#"{
            "type": "system",
            "subtype": "compact_boundary",
            "content": "Conversation compacted",
            "timestamp": "2025-10-20T08:00:00.000Z"
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, ClaudeEntryType::System);
        assert!(entry.should_filter()); // compact_boundary should be filtered
    }

    #[test]
    fn test_filter_summary() {
        let json = r#"{
            "type": "summary",
            "uuid": "summary-123",
            "timestamp": "2025-10-20T08:00:00.000Z",
            "message": {"role": "assistant", "content": "Session summary"}
        }"#;

        let entry: ClaudeEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, ClaudeEntryType::Summary);
        assert!(entry.should_filter());
    }
}
