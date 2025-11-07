//! Debug utilities for inspecting Cursor protobuf data
//!
//! This module provides tools for analyzing and debugging Cursor's
//! protobuf message format stored in SQLite databases.
#![allow(dead_code)] // Development tools, not used in production but kept for debugging

use super::db;
use super::protobuf::{CursorBlob, CursorBlobDirectContent, ComplexMessage, ContentBlock};
use prost::Message;
use std::path::Path;

/// Get content with fallback to direct content decoding for user messages
///
/// Assistant messages have Field 1 as ContentWrapper (nested), this method handles that.
/// User messages have Field 1 as direct string, so we decode raw_data as CursorBlobDirectContent.
fn get_content_with_raw_fallback(blob: &CursorBlob, raw_data: &[u8]) -> String {
    // Try content_wrapper first (assistant messages)
    if let Some(wrapper) = &blob.content_wrapper {
        if let Some(text) = &wrapper.text {
            if !text.is_empty() {
                return text.clone();
            }
        }
    }

    // Fall back to decoding as direct content (user messages)
    if let Ok(direct_blob) = CursorBlobDirectContent::decode(raw_data) {
        if let Some(content) = direct_blob.content {
            if !content.is_empty() {
                return content;
            }
        }
    }

    String::new()
}

/// Inspect a single blob by hex-encoded data
pub fn inspect_blob_hex(hex_data: &str) -> Result<(), Box<dyn std::error::Error>> {
    let data = hex::decode(hex_data)?;
    inspect_blob_bytes(&data)
}

/// Inspect a single blob by raw bytes
pub fn inspect_blob_bytes(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let blob = CursorBlob::decode_from_bytes(data)?;

    println!("=== Cursor Blob Inspection ===");
    println!("Field 1 (content_wrapper): {:?}", blob.content_wrapper);
    println!("Field 2 (uuid): {:?}", blob.uuid);
    println!("Field 3 (metadata): {:?}", blob.metadata);
    println!("Field 4 (complex_data): {:?}", blob.complex_data.as_ref().map(|s| &s[..s.len().min(200)]));
    println!("Field 5 (additional_content): {:?}", blob.additional_content);
    println!("Field 8 (blob_references): {} bytes", blob.blob_references.as_ref().map(|b| b.len()).unwrap_or(0));

    // Parse complex data if present
    if let Some(complex) = blob.parse_complex() {
        println!("\n=== Complex Message ===");
        println!("ID: {}", complex.id);
        println!("Role: {}", complex.role);
        println!("Content blocks: {}", complex.content.len());

        for (i, block) in complex.content.iter().enumerate() {
            println!("\n  Block {}: {}", i, block_type_name(block));
            match block {
                ContentBlock::Text { text } => {
                    println!("    Text: {:?}", &text[..text.len().min(100)]);
                }
                ContentBlock::ToolCall { tool_call_id, tool_name, args } => {
                    println!("    Tool Call ID: {}", tool_call_id);
                    println!("    Tool Name: {}", tool_name);
                    println!("    Args: {}", serde_json::to_string_pretty(args)?);
                }
                ContentBlock::ToolResult { tool_call_id, output, is_error } => {
                    println!("    Tool Call ID: {}", tool_call_id);
                    println!("    Output: {:?}", &output[..output.len().min(100)]);
                    println!("    Is Error: {}", is_error);
                }
                ContentBlock::RedactedReasoning { data } => {
                    println!("    Data: {} bytes", data.len());
                }
            }
        }
    }

    // Parse additional content if present
    if let Some(additional) = blob.parse_additional_content() {
        println!("\n=== Additional Content (Field 5) ===");
        println!("{}", serde_json::to_string_pretty(&additional)?);
    }

    Ok(())
}

/// Inspect all blobs in a Cursor session database (using hybrid decoder)
pub fn inspect_session_db(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Inspecting Cursor Session ===");
    println!("Database: {}", db_path.display());

    let conn = db::open_cursor_db(db_path)?;

    // Get metadata
    match db::get_session_metadata(&conn) {
        Ok(metadata) => {
            println!("\n=== Session Metadata ===");
            println!("Agent ID: {}", metadata.agent_id);
            println!("Name: {}", metadata.name);
            println!("Mode: {}", metadata.mode);
            println!("Created: {}", metadata.created_at);
            println!("Last Model: {}", metadata.last_used_model);
            println!("Latest Root Blob: {}", metadata.latest_root_blob_id);
        }
        Err(e) => {
            println!("Failed to get metadata: {:?}", e);
        }
    }

    // Get all blobs using hybrid decoder
    let messages = db::get_decoded_messages(&conn)?;
    let total_blobs = db::get_blob_count(&conn)? as usize;

    println!("\n=== Hybrid Decoder Results ===");
    println!("Total blobs: {}", total_blobs);
    println!("Successfully decoded: {}", messages.len());
    println!("Success rate: {:.1}%", (messages.len() as f64 / total_blobs as f64) * 100.0);

    // Count by type
    let protobuf_count = messages.iter().filter(|(_, _, msg)| matches!(msg, super::protobuf::CursorMessage::Protobuf(_))).count();
    let json_count = messages.iter().filter(|(_, _, msg)| matches!(msg, super::protobuf::CursorMessage::Json(_))).count();

    println!("\nMessage types:");
    println!("  Protobuf: {} ({:.1}%)", protobuf_count, (protobuf_count as f64 / messages.len() as f64) * 100.0);
    println!("  JSON: {} ({:.1}%)", json_count, (json_count as f64 / messages.len() as f64) * 100.0);

    println!("\n=== Message Details ===");
    for (i, (id, raw_data, msg)) in messages.iter().enumerate() {
        println!("\n--- Message {} ---", i + 1);
        println!("ID: {}", id);

        match msg {
            super::protobuf::CursorMessage::Protobuf(blob) => {
                println!("Type: Protobuf");
                print_blob_summary(blob, raw_data);
            }
            super::protobuf::CursorMessage::Json(json_msg) => {
                println!("Type: JSON");
                println!("  ID: {}", json_msg.id);
                println!("  Role: {}", json_msg.role);
                println!("  Content: {:?}", truncate(&json_msg.content.to_string(), 100));
            }
        }
    }

    Ok(())
}

/// Print a summary of a blob (without full details)
fn print_blob_summary(blob: &CursorBlob, raw_data: &[u8]) {
    println!("  Is Message Blob: {}", blob.is_message_blob());

    // Use raw data fallback to get correct content
    let content = get_content_with_raw_fallback(blob, raw_data);
    println!("  Content: {:?}", truncate(&content, 50));
    println!("  UUID: {}", blob.get_uuid());
    println!("  Role: {}", blob.get_role());

    if blob.is_complex() {
        if let Some(complex) = blob.parse_complex() {
            println!("  Complex: {} blocks", complex.content.len());
            for block in &complex.content {
                println!("    - {}", block_type_name(block));
            }
        }
    }

    if blob.has_tool_result() {
        println!("  ⚠️  Contains tool result");
    }

    if let Some(blob_refs) = &blob.blob_references {
        println!("  Blob References: {} bytes", blob_refs.len());
    }
}

/// Find tool use examples in a database
pub fn find_tool_use_examples(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Searching for Tool Use Examples ===");

    let conn = db::open_cursor_db(db_path)?;
    let blobs = db::get_all_blobs(&conn)?;

    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();

    for (id, data) in blobs {
        if let Ok(blob) = CursorBlob::decode_from_bytes(&data) {
            if let Some(complex) = blob.parse_complex() {
                for block in &complex.content {
                    match block {
                        ContentBlock::ToolCall { tool_call_id, tool_name, .. } => {
                            tool_calls.push((id.clone(), tool_call_id.clone(), tool_name.clone()));
                        }
                        ContentBlock::ToolResult { tool_call_id, .. } => {
                            tool_results.push((id.clone(), tool_call_id.clone()));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    println!("\n=== Results ===");
    println!("Tool Calls: {}", tool_calls.len());
    for (blob_id, call_id, name) in &tool_calls {
        println!("  - {} (call_id: {}) in blob {}", name, call_id, &blob_id[..16]);
    }

    println!("\nTool Results: {}", tool_results.len());
    for (blob_id, call_id) in &tool_results {
        println!("  - Result for {} in blob {}", call_id, &blob_id[..16]);
    }

    Ok(())
}

/// Export all blobs as JSON for analysis
pub fn export_blobs_json(db_path: &Path, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let conn = db::open_cursor_db(db_path)?;
    let decoded_blobs = db::get_decoded_blobs(&conn)?;

    #[derive(serde::Serialize)]
    struct BlobExport {
        id: String,
        content: String,
        uuid: String,
        metadata: String,
        complex_data: Option<ComplexMessage>,
        additional_content: Option<serde_json::Value>,
        role: String,
        is_complex: bool,
        has_tool_result: bool,
    }

    let exports: Vec<BlobExport> = decoded_blobs
        .into_iter()
        .map(|(id, blob)| BlobExport {
            content: blob.get_content(),
            uuid: blob.get_uuid().to_string(),
            metadata: blob.metadata.clone().unwrap_or_default(),
            complex_data: blob.parse_complex(),
            additional_content: blob.parse_additional_content(),
            role: blob.get_role(),
            is_complex: blob.is_complex(),
            has_tool_result: blob.has_tool_result(),
            id,
        })
        .collect();

    let json = serde_json::to_string_pretty(&exports)?;
    std::fs::write(output_path, json)?;

    println!("Exported {} blobs to {}", exports.len(), output_path.display());

    Ok(())
}

// Helper functions

fn block_type_name(block: &ContentBlock) -> &str {
    match block {
        ContentBlock::Text { .. } => "Text",
        ContentBlock::ToolCall { .. } => "ToolCall",
        ContentBlock::ToolResult { .. } => "ToolResult",
        ContentBlock::RedactedReasoning { .. } => "RedactedReasoning",
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspect_sample_blobs() {
        // Sample hex data from real Cursor sessions
        let samples = vec![
            // Simple user message
            "0A4B43616E20796F752072756E20746865206C696E74657220666F7220617070732F73657276657220616E642074656C6C206D6520696620746865726520697320776F726B20746F20646F3F20122435623936353366642D663035342D343132362D3966",
            // Assistant response
            "0A3D0A3B0A52756E6E696E6720746865206C696E74657220666F722060617070732F7365727665726020746F20636865636B20666F72206973737565732E0A",
        ];

        for (i, hex) in samples.iter().enumerate() {
            println!("\n=== Testing sample {} ===", i);
            if let Err(e) = inspect_blob_hex(hex) {
                println!("Failed: {:?}", e);
            }
        }
    }
}
