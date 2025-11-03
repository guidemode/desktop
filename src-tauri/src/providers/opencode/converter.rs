use crate::providers::canonical::{
    CanonicalMessage, ContentBlock, ContentValue, MessageContent, MessageType,
};
use crate::providers::opencode_parser::{OpenCodeJsonLContent, OpenCodeJsonLEntry};
use anyhow::{Context, Result};
use uuid::Uuid;

/// Convert OpenCode aggregated JSONL entry to canonical format
///
/// OpenCode's parser already does the hard work of aggregating session,
/// message, and part files into JSONL. This converter just transforms
/// the aggregated format to canonical.
pub fn convert_entry_to_canonical(entry: &OpenCodeJsonLEntry) -> Result<CanonicalMessage> {
    // Determine message type from entry_type
    let message_type = match entry.entry_type.as_str() {
        "user" => MessageType::User,
        "assistant" => MessageType::Assistant,
        "tool_use" => MessageType::Assistant,
        "tool_result" => MessageType::User,  // Tool results are USER messages
        _ => MessageType::Meta,
    };

    // Determine role - tool results should be "user" not "tool"
    let role = if entry.entry_type == "tool_result" {
        "user".to_string()  // Tool results have user role
    } else {
        entry.message.role.clone()
    };

    // Convert content blocks
    let content = convert_content_blocks(&entry.message.content)?;

    // Generate UUID if needed (some entries might not have unique IDs)
    let uuid = if entry.session_id.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        // Create a deterministic ID from session + timestamp
        format!("{}-{}", entry.session_id, entry.timestamp)
    };

    Ok(CanonicalMessage {
        uuid,
        timestamp: entry.timestamp.clone(),
        message_type,
        session_id: entry.session_id.clone(),
        provider: "opencode".to_string(),
        cwd: entry.cwd.clone(),
        git_branch: None,
        version: None,
        parent_uuid: None,
        is_sidechain: None,
        user_type: Some("external".to_string()),
        message: MessageContent {
            role,
            content,
            model: None,
            usage: None,
        },
        provider_metadata: Some(serde_json::json!({
            "opencode_type": entry.entry_type,
        })),
        is_meta: None,
        request_id: None,
        tool_use_result: None,
    })
}

/// Convert OpenCode content blocks to canonical format
fn convert_content_blocks(blocks: &[OpenCodeJsonLContent]) -> Result<ContentValue> {
    if blocks.is_empty() {
        return Ok(ContentValue::Text(String::new()));
    }

    // If there's only one text block, return it as plain text
    if blocks.len() == 1 {
        if let OpenCodeJsonLContent::Text { text, .. } = &blocks[0] {
            return Ok(ContentValue::Text(text.clone()));
        }
    }

    // Otherwise, convert to structured content blocks
    let canonical_blocks: Vec<ContentBlock> = blocks
        .iter()
        .filter_map(|block| convert_content_block(block).ok())
        .collect();

    if canonical_blocks.is_empty() {
        Ok(ContentValue::Text(String::new()))
    } else {
        Ok(ContentValue::Structured(canonical_blocks))
    }
}

/// Convert individual OpenCode content block to canonical format
fn convert_content_block(block: &OpenCodeJsonLContent) -> Result<ContentBlock> {
    match block {
        OpenCodeJsonLContent::Text { text, .. } => Ok(ContentBlock::Text {
            text: text.clone(),
        }),
        OpenCodeJsonLContent::ToolUse {
            id, name, input, ..
        } => Ok(ContentBlock::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        }),
        OpenCodeJsonLContent::ToolResult {
            tool_use_id,
            content,
            is_error,
            ..
        } => {
            // Validate we have required data for tool_result
            // Don't create empty tool_result blocks (causes parsing issues)
            if tool_use_id.is_empty() || content.is_empty() {
                return Err(anyhow::anyhow!(
                    "Tool result missing required fields: tool_use_id='{}', content length={}",
                    tool_use_id,
                    content.len()
                ));
            }

            Ok(ContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                content: content.clone(),
                is_error: *is_error,
            })
        }
        OpenCodeJsonLContent::File {
            filename,
            mime,
            url,
            ..
        } => {
            // Convert file reference to text with metadata
            // Files are OpenCode-specific, so we preserve them as text
            let text = format!(
                "[File: {} ({})] URL: {}",
                filename, mime, url
            );
            Ok(ContentBlock::Text { text })
        }
        OpenCodeJsonLContent::Patch { files, hash, .. } => {
            // Convert patch reference to text with metadata
            // Patches are OpenCode-specific, so we preserve them as text
            let text = format!(
                "[Patch: {} files, hash: {}] Files: {}",
                files.len(),
                hash,
                files.join(", ")
            );
            Ok(ContentBlock::Text { text })
        }
    }
}

/// Convert a complete OpenCode JSONL string to canonical JSONL
pub fn convert_opencode_jsonl_to_canonical(opencode_jsonl: &str) -> Result<String> {
    let mut canonical_lines = Vec::new();

    for (line_num, line) in opencode_jsonl.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let entry: OpenCodeJsonLEntry = serde_json::from_str(line)
            .with_context(|| format!("Failed to parse OpenCode JSONL line {}", line_num))?;

        let canonical = convert_entry_to_canonical(&entry)
            .with_context(|| format!("Failed to convert OpenCode entry at line {}", line_num))?;

        let canonical_line = serde_json::to_string(&canonical)
            .with_context(|| format!("Failed to serialize canonical message at line {}", line_num))?;

        canonical_lines.push(canonical_line);
    }

    Ok(canonical_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::opencode_parser::{OpenCodeJsonLMessage, OpenCodeJsonLContent};

    #[test]
    fn test_convert_text_entry() {
        let entry = OpenCodeJsonLEntry {
            session_id: "test-session".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            entry_type: "user".to_string(),
            message: OpenCodeJsonLMessage {
                role: "user".to_string(),
                content: vec![OpenCodeJsonLContent::Text {
                    content_type: "text".to_string(),
                    text: "Hello, world!".to_string(),
                }],
            },
            cwd: Some("/test/project".to_string()),
        };

        let canonical = convert_entry_to_canonical(&entry).unwrap();

        assert_eq!(canonical.message_type, MessageType::User);
        assert_eq!(canonical.session_id, "test-session");
        assert_eq!(canonical.provider, "opencode");
        assert_eq!(canonical.cwd, Some("/test/project".to_string()));

        match &canonical.message.content {
            ContentValue::Text(text) => assert_eq!(text, "Hello, world!"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_tool_use_entry() {
        let entry = OpenCodeJsonLEntry {
            session_id: "test-session".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            entry_type: "tool_use".to_string(),
            message: OpenCodeJsonLMessage {
                role: "tool".to_string(),
                content: vec![OpenCodeJsonLContent::ToolUse {
                    content_type: "tool_use".to_string(),
                    id: "call_123".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({"path": "/test/file.txt"}),
                }],
            },
            cwd: Some("/test/project".to_string()),
        };

        let canonical = convert_entry_to_canonical(&entry).unwrap();

        assert_eq!(canonical.message_type, MessageType::Assistant);

        match &canonical.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call_123");
                        assert_eq!(name, "read_file");
                    }
                    _ => panic!("Expected tool_use block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_convert_tool_result_entry() {
        let entry = OpenCodeJsonLEntry {
            session_id: "test-session".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            entry_type: "tool_result".to_string(),
            message: OpenCodeJsonLMessage {
                role: "tool".to_string(),
                content: vec![OpenCodeJsonLContent::ToolResult {
                    content_type: "tool_result".to_string(),
                    tool_use_id: "call_123".to_string(),
                    content: "File contents here".to_string(),
                    is_error: Some(false),
                }],
            },
            cwd: Some("/test/project".to_string()),
        };

        let canonical = convert_entry_to_canonical(&entry).unwrap();

        match &canonical.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        assert_eq!(tool_use_id, "call_123");
                        assert_eq!(content, "File contents here");
                        assert_eq!(*is_error, Some(false));
                    }
                    _ => panic!("Expected tool_result block"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_convert_file_entry() {
        let entry = OpenCodeJsonLEntry {
            session_id: "test-session".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            entry_type: "user".to_string(),
            message: OpenCodeJsonLMessage {
                role: "user".to_string(),
                content: vec![OpenCodeJsonLContent::File {
                    content_type: "file".to_string(),
                    filename: "test.png".to_string(),
                    mime: "image/png".to_string(),
                    url: "file:///test/test.png".to_string(),
                }],
            },
            cwd: Some("/test/project".to_string()),
        };

        let canonical = convert_entry_to_canonical(&entry).unwrap();

        match &canonical.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Text { text } => {
                        assert!(text.contains("test.png"));
                        assert!(text.contains("image/png"));
                    }
                    _ => panic!("Expected text block for file"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_convert_patch_entry() {
        let entry = OpenCodeJsonLEntry {
            session_id: "test-session".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            entry_type: "assistant".to_string(),
            message: OpenCodeJsonLMessage {
                role: "assistant".to_string(),
                content: vec![OpenCodeJsonLContent::Patch {
                    content_type: "patch".to_string(),
                    files: vec!["file1.rs".to_string(), "file2.rs".to_string()],
                    hash: "abc123".to_string(),
                }],
            },
            cwd: Some("/test/project".to_string()),
        };

        let canonical = convert_entry_to_canonical(&entry).unwrap();

        match &canonical.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Text { text } => {
                        assert!(text.contains("2 files"));
                        assert!(text.contains("abc123"));
                        assert!(text.contains("file1.rs"));
                    }
                    _ => panic!("Expected text block for patch"),
                }
            }
            _ => panic!("Expected structured content"),
        }
    }

    #[test]
    fn test_convert_mixed_content() {
        let entry = OpenCodeJsonLEntry {
            session_id: "test-session".to_string(),
            timestamp: "2025-01-01T00:00:00.000Z".to_string(),
            entry_type: "user".to_string(),
            message: OpenCodeJsonLMessage {
                role: "user".to_string(),
                content: vec![
                    OpenCodeJsonLContent::Text {
                        content_type: "text".to_string(),
                        text: "Here's a file:".to_string(),
                    },
                    OpenCodeJsonLContent::File {
                        content_type: "file".to_string(),
                        filename: "doc.pdf".to_string(),
                        mime: "application/pdf".to_string(),
                        url: "file:///test/doc.pdf".to_string(),
                    },
                ],
            },
            cwd: Some("/test/project".to_string()),
        };

        let canonical = convert_entry_to_canonical(&entry).unwrap();

        match &canonical.message.content {
            ContentValue::Structured(blocks) => {
                assert_eq!(blocks.len(), 2);
            }
            _ => panic!("Expected structured content"),
        }
    }
}
