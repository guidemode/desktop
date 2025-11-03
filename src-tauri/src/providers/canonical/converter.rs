use super::CanonicalMessage;
use anyhow::Result;

/// Trait for converting provider-specific formats to canonical JSONL
///
/// Each provider implements this trait to convert their native format
/// to the unified canonical format.
pub trait ToCanonical {
    /// Convert provider's raw message to canonical format
    ///
    /// This method should:
    /// 1. Map provider-specific fields to canonical fields
    /// 2. Preserve provider-specific data in `provider_metadata`
    /// 3. Handle missing fields gracefully (use None for optional fields)
    /// 4. Return None for messages that should be skipped (e.g., duplicates)
    fn to_canonical(&self) -> Result<Option<CanonicalMessage>>;

    /// Provider name (e.g., "codex", "gemini-code", "github-copilot")
    fn provider_name(&self) -> &str;

    /// Extract CWD from message (if not in dedicated field)
    ///
    /// Some providers (Gemini, Copilot, OpenCode) don't have a dedicated
    /// CWD field and need to infer it from message content.
    fn extract_cwd(&self) -> Option<String> {
        None
    }

    /// Extract git branch (if available)
    ///
    /// Most providers don't track git branch, so this defaults to None.
    fn extract_git_branch(&self) -> Option<String> {
        None
    }

    /// Extract provider version (if available)
    fn extract_version(&self) -> Option<String> {
        None
    }
}

/// Batch conversion helper for converting multiple messages
/// Filters out None values (skipped messages)
#[allow(dead_code)]
pub fn convert_batch<T: ToCanonical>(
    messages: Vec<T>,
) -> Result<Vec<CanonicalMessage>> {
    messages
        .into_iter()
        .filter_map(|msg| match msg.to_canonical() {
            Ok(Some(canonical)) => Some(Ok(canonical)),
            Ok(None) => None, // Skip this message
            Err(e) => Some(Err(e)),
        })
        .collect()
}

/// Convert messages and serialize to JSONL format
#[allow(dead_code)]
pub fn to_jsonl<T: ToCanonical>(messages: Vec<T>) -> Result<String> {
    let canonical_messages = convert_batch(messages)?;
    let lines: Result<Vec<String>> = canonical_messages
        .iter()
        .map(|msg| serde_json::to_string(msg).map_err(Into::into))
        .collect();

    Ok(lines?.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::canonical::{ContentValue, MessageContent, MessageType};

    // Mock implementation for testing
    struct MockMessage {
        id: String,
        text: String,
    }

    impl ToCanonical for MockMessage {
        fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
            Ok(Some(CanonicalMessage {
                uuid: self.id.clone(),
                timestamp: "2025-01-01T00:00:00.000Z".to_string(),
                message_type: MessageType::User,
                session_id: "test-session".to_string(),
                provider: self.provider_name().to_string(),
                cwd: self.extract_cwd(),
                git_branch: None,
                version: None,
                parent_uuid: None,
                is_sidechain: None,
                user_type: Some("external".to_string()),
                message: MessageContent {
                    role: "user".to_string(),
                    content: ContentValue::Text(self.text.clone()),
                    model: None,
                    usage: None,
                },
                provider_metadata: None,
                is_meta: None,
                request_id: None,
                tool_use_result: None,
            }))
        }

        fn provider_name(&self) -> &str {
            "mock-provider"
        }

        fn extract_cwd(&self) -> Option<String> {
            Some("/mock/path".to_string())
        }
    }

    #[test]
    fn test_convert_batch() {
        let messages = vec![
            MockMessage {
                id: "1".to_string(),
                text: "First message".to_string(),
            },
            MockMessage {
                id: "2".to_string(),
                text: "Second message".to_string(),
            },
        ];

        let canonical = convert_batch(messages).unwrap();

        assert_eq!(canonical.len(), 2);
        assert_eq!(canonical[0].uuid, "1");
        assert_eq!(canonical[1].uuid, "2");
        assert_eq!(canonical[0].provider, "mock-provider");
    }

    #[test]
    fn test_to_jsonl() {
        let messages = vec![
            MockMessage {
                id: "msg-1".to_string(),
                text: "Test".to_string(),
            },
            MockMessage {
                id: "msg-2".to_string(),
                text: "Test 2".to_string(),
            },
        ];

        let jsonl = to_jsonl(messages).unwrap();

        // Should have two lines
        let lines: Vec<&str> = jsonl.split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON
        let msg1: CanonicalMessage = serde_json::from_str(lines[0]).unwrap();
        let msg2: CanonicalMessage = serde_json::from_str(lines[1]).unwrap();

        assert_eq!(msg1.uuid, "msg-1");
        assert_eq!(msg2.uuid, "msg-2");
    }

    #[test]
    fn test_extract_cwd_default() {
        struct NoCwdMessage;

        impl ToCanonical for NoCwdMessage {
            fn to_canonical(&self) -> Result<Option<CanonicalMessage>> {
                Ok(Some(CanonicalMessage::new_text_message(
                    "id".to_string(),
                    "2025-01-01T00:00:00.000Z".to_string(),
                    MessageType::User,
                    "session".to_string(),
                    "test".to_string(),
                    "user".to_string(),
                    "text".to_string(),
                )))
            }

            fn provider_name(&self) -> &str {
                "test"
            }
        }

        let msg = NoCwdMessage;
        assert_eq!(msg.extract_cwd(), None);
    }
}
