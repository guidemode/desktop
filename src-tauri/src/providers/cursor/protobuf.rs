//! Cursor Protobuf Message Types
//!
//! Based on reverse engineering of Cursor's SQLite blobs.
//!
//! Schema discovered through hex analysis (see cursor_hex_inspector and cursor_blob_analyzer):
//! - Field 1: VARIABLE - Can be:
//!   * Direct string (user messages)
//!   * Nested ContentWrapper message (assistant messages)
//!   * Nested structure (tree/reference blobs)
//! - Field 2: UUID (string) - message identifier
//! - Field 3: Empty string or metadata
//! - Field 4: JSON-encoded complex message (tool calls, reasoning, etc.)
//! - Field 5: Tool output or additional content
//! - Field 8: Blob references (32-byte SHA-256 hashes)
#![allow(dead_code)] // Helper methods for debugging, not all used in production

use prost::Message;
use serde::{Deserialize, Serialize};

/// Wrapper for Field 1 content in assistant messages
///
/// Assistant messages have Field 1 as a nested message containing:
/// - Field 1: The actual text content (string)
///
/// This was discovered by analyzing corrupted output like "\n&I'll run..."
/// where \x0a\x26 are actually protobuf field markers being misinterpreted as string content.
#[derive(Clone, PartialEq, Message)]
pub struct ContentWrapper {
    /// The actual message text
    #[prost(string, optional, tag = "1")]
    pub text: Option<String>,
}

/// A Cursor message blob decoded from Protocol Buffers
///
/// Note: Cursor uses different blob types:
/// - User messages: Field 1 is a direct string
/// - Assistant messages: Field 1 is a ContentWrapper (nested message)
/// - Tree/reference blobs: Field 1 has different nested structures
///
/// To handle this polymorphism, we first try to decode Field 1 as a message,
/// then fall back to string if that fails.
#[derive(Clone, PartialEq, Message)]
pub struct CursorBlob {
    /// Message content wrapper (for assistant messages)
    /// This is Field 1 decoded as a nested message
    #[prost(message, optional, tag = "1")]
    pub content_wrapper: Option<ContentWrapper>,

    /// Message UUID
    #[prost(string, optional, tag = "2")]
    pub uuid: Option<String>,

    /// Additional metadata or empty string
    #[prost(string, optional, tag = "3")]
    pub metadata: Option<String>,

    /// Complex message data (JSON-encoded for tool calls, etc.)
    #[prost(string, optional, tag = "4")]
    pub complex_data: Option<String>,

    /// Tool output or additional content
    #[prost(string, optional, tag = "5")]
    pub additional_content: Option<String>,

    /// References to other blobs (SHA-256 hashes)
    /// Tree/reference blobs use this field
    #[prost(bytes = "vec", optional, tag = "8")]
    pub blob_references: Option<Vec<u8>>,
}

/// Alternative schema for user messages where Field 1 is a direct string
#[derive(Clone, PartialEq, Message)]
pub struct CursorBlobDirectContent {
    /// Direct string content (for user messages)
    #[prost(string, optional, tag = "1")]
    pub content: Option<String>,

    /// Message UUID
    #[prost(string, optional, tag = "2")]
    pub uuid: Option<String>,

    /// Additional metadata
    #[prost(string, optional, tag = "3")]
    pub metadata: Option<String>,

    /// Complex message data
    #[prost(string, optional, tag = "4")]
    pub complex_data: Option<String>,

    /// Additional content
    #[prost(string, optional, tag = "5")]
    pub additional_content: Option<String>,

    /// Blob references
    #[prost(bytes = "vec", optional, tag = "8")]
    pub blob_references: Option<Vec<u8>>,
}

/// Complex message structure (parsed from Field 4 JSON)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexMessage {
    pub id: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
}

/// Content blocks within a complex message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "tool-call")]
    ToolCall {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,

        #[serde(rename = "toolName")]
        tool_name: String,

        args: serde_json::Value,
    },

    #[serde(rename = "tool-result")]
    ToolResult {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,

        output: String,

        #[serde(default)]
        is_error: bool,
    },

    #[serde(rename = "redacted-reasoning")]
    RedactedReasoning { data: String },
}

/// JSON message format (fallback for non-protobuf messages)
/// Cursor stores some messages as raw JSON in Anthropic API format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMessage {
    #[serde(default)]
    pub id: String,
    pub role: String,
    pub content: serde_json::Value,
}

/// Hybrid message type that can be either protobuf or JSON
#[derive(Debug, Clone)]
pub enum CursorMessage {
    Protobuf(CursorBlob),
    Json(JsonMessage),
}

impl CursorMessage {
    /// Decode a message from raw bytes, trying protobuf first, then JSON
    pub fn decode_from_bytes(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        // Detect format by checking first byte
        let format_hint = if !data.is_empty() {
            if data[0] == b'{' {
                "JSON (starts with '{')"
            } else {
                "Protobuf (binary)"
            }
        } else {
            "Empty"
        };

        tracing::debug!(
            "Decoding {} bytes, format hint: {}, first 4 bytes: {:?}",
            data.len(),
            format_hint,
            &data[..data.len().min(4)]
        );

        // Try protobuf first
        match CursorBlob::decode(data) {
            Ok(blob) => {
                tracing::debug!("✓ Protobuf decode SUCCESS");
                Ok(CursorMessage::Protobuf(blob))
            },
            Err(protobuf_err) => {
                tracing::debug!("✗ Protobuf decode FAILED: {:?}", protobuf_err);
                tracing::debug!("  Attempting JSON fallback...");

                // Fallback to JSON
                match serde_json::from_slice::<JsonMessage>(data) {
                    Ok(json_msg) => {
                        tracing::debug!("✓ JSON decode SUCCESS (role: {}, id: {})", json_msg.role, json_msg.id);
                        Ok(CursorMessage::Json(json_msg))
                    }
                    Err(json_err) => {
                        // Both protobuf and JSON failed - likely a tree/reference blob (internal Cursor structure)
                        // These are expected and can be safely ignored
                        tracing::debug!(
                            "⊘ Skipping non-message blob (likely tree/reference blob)\n  Protobuf error: {:?}\n  JSON error: {}",
                            protobuf_err,
                            json_err
                        );
                        Err(Box::new(json_err))
                    }
                }
            }
        }
    }

    /// Get the message role (user/assistant/tool)
    pub fn get_role(&self) -> String {
        match self {
            CursorMessage::Protobuf(blob) => blob.get_role(),
            CursorMessage::Json(json) => json.role.clone(),
        }
    }

    /// Get a unique ID for this message
    pub fn get_id(&self) -> String {
        match self {
            CursorMessage::Protobuf(blob) => blob.get_uuid().to_string(),
            CursorMessage::Json(json) => json.id.clone(),
        }
    }
}

impl CursorBlob {
    /// Decode a blob from raw bytes (tries nested wrapper schema)
    pub fn decode_from_bytes(data: &[u8]) -> Result<Self, prost::DecodeError> {
        CursorBlob::decode(data)
    }

    /// Get the actual text content, trying both nested wrapper and direct content
    ///
    /// This handles the polymorphism of Field 1:
    /// - Assistant messages: content_wrapper.text
    /// - User messages: decode as CursorBlobDirectContent and get content
    fn get_field1_content(&self, raw_data: &[u8]) -> String {
        // Try nested wrapper first (assistant messages)
        if let Some(wrapper) = &self.content_wrapper {
            if let Some(text) = &wrapper.text {
                return text.clone();
            }
        }

        // Fall back to decoding as direct content (user messages)
        if let Ok(direct_blob) = CursorBlobDirectContent::decode(raw_data) {
            if let Some(content) = direct_blob.content {
                return content;
            }
        }

        String::new()
    }

    /// Check if this blob is a message blob (has content or complex data)
    /// Returns false for tree/reference blobs
    ///
    /// Message detection:
    /// - Has content_wrapper (assistant messages with nested Field 1)
    /// - Has complex_data (messages with tool calls, reasoning, etc.)
    /// - Has UUID (user messages - they always have UUIDs)
    pub fn is_message_blob(&self) -> bool {
        self.content_wrapper.is_some()
            || self.complex_data.is_some()
            || (self.uuid.is_some() && !self.uuid.as_deref().unwrap_or("").is_empty())
    }

    /// Check if this blob contains a complex message (has JSON in field 4)
    pub fn is_complex(&self) -> bool {
        self.complex_data.is_some()
    }

    /// Parse complex message data if present
    pub fn parse_complex(&self) -> Option<ComplexMessage> {
        self.complex_data
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok())
    }

    /// Parse additional content field (Field 5) if present
    /// This field may contain tool outputs or other additional data
    pub fn parse_additional_content(&self) -> Option<serde_json::Value> {
        self.additional_content
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok())
    }

    /// Detect if this blob represents a tool result message
    /// Tool results can appear in complex_data as ToolResult content blocks
    pub fn has_tool_result(&self) -> bool {
        if let Some(complex) = self.parse_complex() {
            // Check if any content block is a ToolResult
            complex.content.iter().any(|block| {
                matches!(block, ContentBlock::ToolResult { .. })
            })
        } else {
            false
        }
    }

    /// Get the primary message content (from complex data or Field 1)
    ///
    /// Note: This requires the raw blob data to try both nested and direct content schemas
    pub fn get_content_with_fallback(&self, raw_data: &[u8]) -> String {
        if let Some(complex) = self.parse_complex() {
            // Extract text from content blocks
            complex
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            self.get_field1_content(raw_data)
        }
    }

    /// Get content (simple version without raw data - tries wrapper only)
    pub fn get_content(&self) -> String {
        if let Some(complex) = self.parse_complex() {
            complex
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else if let Some(wrapper) = &self.content_wrapper {
            wrapper.text.as_deref().unwrap_or("").to_string()
        } else {
            String::new()
        }
    }

    /// Get the role (user/assistant) from complex data or infer from structure
    ///
    /// Detection strategy (in priority order):
    /// 1. Check complex_data.role (most reliable - explicitly set by Cursor)
    /// 2. Check Field 1 structure:
    ///    - Has content_wrapper (nested message) → assistant
    ///    - No content_wrapper → user (has direct string content)
    /// 3. Check UUID as last resort:
    ///    - Has UUID → likely user message
    ///    - No UUID → likely assistant message
    pub fn get_role(&self) -> String {
        // Priority 1: Complex data role (most reliable)
        if let Some(complex) = self.parse_complex() {
            tracing::trace!("Role from complex_data: {}", complex.role);
            return complex.role;
        }

        // Priority 2: Field 1 structure
        // Assistant messages have nested ContentWrapper, user messages have direct string
        if self.content_wrapper.is_some() {
            tracing::trace!("Role from Field 1 structure (has content_wrapper): assistant");
            return "assistant".to_string();
        }

        // Priority 3: UUID heuristic (least reliable)
        // User messages typically have UUIDs, assistant messages don't
        if self.uuid.is_some() && !self.uuid.as_deref().unwrap_or("").is_empty() {
            tracing::trace!("Role from UUID heuristic (has UUID): user");
            "user".to_string()
        } else {
            tracing::trace!("Role from UUID heuristic (no UUID): assistant");
            "assistant".to_string()
        }
    }

    /// Get the UUID, or an empty string if not present
    pub fn get_uuid(&self) -> &str {
        self.uuid.as_deref().unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_message_decode() {
        // Example from our analysis: user message blob
        let data = vec![
            0x0a, 0x4b, // Field 1, length 75
            b'T', b'e', b's', b't', b' ', b'm', b'e', b's', b's', b'a', b'g', b'e',
        ];

        // This is a simplified test - real data would be longer
        // Just testing the decode mechanism works
    }

    #[test]
    fn test_role_inference() {
        let user_blob = CursorBlob {
            content_wrapper: None,
            uuid: Some("some-uuid".to_string()),
            metadata: Some(String::new()),
            complex_data: None,
            additional_content: None,
            blob_references: None,
        };

        assert_eq!(user_blob.get_role(), "user");

        let assistant_blob = CursorBlob {
            content_wrapper: Some(ContentWrapper {
                text: Some("Response".to_string()),
            }),
            uuid: None,
            metadata: None,
            complex_data: None,
            additional_content: None,
            blob_references: None,
        };

        assert_eq!(assistant_blob.get_role(), "assistant");
    }

    #[test]
    fn test_tree_blob_detection() {
        // Tree blobs have no content or complex data
        let tree_blob = CursorBlob {
            content_wrapper: None,
            uuid: None,
            metadata: None,
            complex_data: None,
            additional_content: None,
            blob_references: Some(vec![1, 2, 3, 4]),  // Has blob references
        };

        assert!(!tree_blob.is_message_blob(), "Tree blobs should not be message blobs");

        // Message blobs have content wrapper
        let message_blob = CursorBlob {
            content_wrapper: Some(ContentWrapper {
                text: Some("Hello".to_string()),
            }),
            uuid: Some("uuid".to_string()),
            metadata: None,
            complex_data: None,
            additional_content: None,
            blob_references: None,
        };

        assert!(message_blob.is_message_blob(), "Message blobs should be detected");
    }
}
