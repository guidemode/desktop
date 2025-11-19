#![allow(warnings)] // Development tool
/// Cursor Blob Analyzer
///
/// High-level blob analysis tool for understanding Cursor message structure.
///
/// Usage:
///   # Analyze a blob from database
///   cargo run --bin cursor_blob_analyzer -- --db <path> --blob-id <id>
///
///   # Analyze hex data directly
///   cargo run --bin cursor_blob_analyzer -- --hex <hex_data>
///
/// This tool provides:
/// - Blob type classification (user message, assistant message, tool use, tree blob)
/// - Field-by-field breakdown
/// - Corruption detection
/// - Comparison with expected schema

use guidemode_desktop::providers::cursor::{db, protobuf::CursorMessage};
use rusqlite::Connection;
use std::path::PathBuf;

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .with_target(false)
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let result = if args[1] == "--db" {
        if args.len() < 5 || args[3] != "--blob-id" {
            eprintln!("Error: --db requires --blob-id");
            print_usage();
            std::process::exit(1);
        }
        let db_path = PathBuf::from(&args[2]);
        let blob_id = &args[4];
        analyze_blob_from_db(&db_path, blob_id)
    } else if args[1] == "--hex" {
        if args.len() < 3 {
            eprintln!("Error: --hex requires hex data");
            print_usage();
            std::process::exit(1);
        }
        analyze_hex(&args[2])
    } else {
        eprintln!("Error: Unknown option '{}'", args[1]);
        print_usage();
        std::process::exit(1);
    };

    if let Err(e) = result {
        eprintln!("Error: {:?}", e);
        std::process::exit(1);
    }
}

fn analyze_blob_from_db(
    db_path: &std::path::Path,
    blob_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Cursor Blob Analyzer ===\n");
    println!("Database: {}", db_path.display());
    println!("Blob ID: {}\n", blob_id);

    let conn = Connection::open(db_path)?;
    let data: Vec<u8> = conn.query_row(
        "SELECT data FROM blobs WHERE id = ?",
        [blob_id],
        |row| row.get(0),
    )?;

    analyze_blob(&data, blob_id)
}

fn analyze_hex(hex_data: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Cursor Blob Analyzer ===\n");

    let data = hex::decode(hex_data)?;
    analyze_blob(&data, "from-hex")
}

fn analyze_blob(data: &[u8], blob_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Blob size: {} bytes\n", data.len());

    // Classify blob type
    let blob_type = classify_blob(data);
    println!("=== Blob Classification ===");
    println!("Type: {}\n", blob_type);

    // Try to decode with current schema
    println!("=== Decoding Attempt ===");
    match CursorMessage::decode_from_bytes(data) {
        Ok(msg) => {
            println!("✓ Decoded successfully");
            println!("Format: {}", match &msg {
                guidemode_desktop::providers::cursor::protobuf::CursorMessage::Protobuf(_) => "Protobuf",
                guidemode_desktop::providers::cursor::protobuf::CursorMessage::Json(_) => "JSON",
            });
            println!("Role: {}", msg.get_role());
            println!("ID: {}\n", msg.get_id());

            // Check for corruption
            detect_corruption(&msg);
        }
        Err(e) => {
            println!("✗ Decode failed: {:?}\n", e);
        }
    }

    // Manual protobuf analysis
    println!("=== Manual Protobuf Analysis ===");
    analyze_protobuf_structure(data)?;

    // Recommendations
    println!("\n=== Recommendations ===");
    provide_recommendations(data, blob_type);

    Ok(())
}

/// Classify blob by examining structure
fn classify_blob(data: &[u8]) -> &'static str {
    if data.is_empty() {
        return "empty";
    }

    // Check if JSON
    if data[0] == b'{' {
        return "json-message";
    }

    // Check protobuf structure
    if data.len() < 2 {
        return "unknown";
    }

    let first_byte = data[0];
    let field_num = first_byte >> 3;
    let wire_type = first_byte & 0x07;

    match (field_num, wire_type) {
        (1, 2) => {
            // Field 1, length-delimited
            // Check if nested message or direct string
            if data.len() > 2 && data[2] == 0x0a {
                "assistant-message-nested"
            } else {
                "user-message-direct"
            }
        }
        (3, 2) => "metadata-blob",
        _ => "tree-or-reference-blob",
    }
}

/// Detect corruption patterns
fn detect_corruption(msg: &CursorMessage) -> bool {
    use guidemode_desktop::providers::cursor::protobuf::CursorMessage;

    match msg {
        CursorMessage::Protobuf(blob) => {
            let content = blob.get_content();

            // Check for protobuf markers in string content
            let corruption_patterns = [
                "\\n&", "\\nK", "\\n3", "\\n,", // Common protobuf field markers as strings
                "\n&", "\nK", "\n3", "\n,",      // Actual control characters
            ];

            for pattern in &corruption_patterns {
                if content.contains(pattern) {
                    println!("⚠️  CORRUPTION DETECTED: Content contains protobuf markers");
                    println!("   Pattern found: {:?}", pattern);
                    println!("   This indicates Field 1 is a nested message, not a string");
                    println!("   Content preview: {:?}\n", &content[..content.len().min(100)]);
                    return true;
                }
            }

            false
        }
        CursorMessage::Json(_) => false,
    }
}

/// Analyze protobuf structure manually
fn analyze_protobuf_structure(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut offset = 0;

    while offset < data.len() {
        if offset >= data.len() {
            break;
        }

        let key = data[offset];
        let field_number = key >> 3;
        let wire_type = key & 0x07;

        println!("Field {} (wire_type {}):", field_number, wire_type);
        offset += 1;

        match wire_type {
            2 => {
                // Length-delimited
                let (length, length_bytes) = read_varint(data, offset)?;
                offset += length_bytes;

                if offset + length as usize > data.len() {
                    println!("  ERROR: Truncated field");
                    break;
                }

                let field_data = &data[offset..offset + length as usize];

                // Try as string
                if let Ok(s) = std::str::from_utf8(field_data) {
                    println!("  As string: {:?}", &s[..s.len().min(100)]);

                    // Check if looks like nested message
                    if !field_data.is_empty() && field_data[0] < 0x20 {
                        println!("  ⚠️  Starts with 0x{:02x} - likely nested message!", field_data[0]);

                        // Try to parse as nested
                        if let Ok(_) = analyze_nested_message(field_data) {
                            println!("  ✓ Successfully parsed as nested message");
                        }
                    }
                } else {
                    println!("  Binary data ({} bytes)", length);
                }

                offset += length as usize;
            }
            0 => {
                // Varint
                let (value, value_bytes) = read_varint(data, offset)?;
                println!("  Varint: {}", value);
                offset += value_bytes;
            }
            _ => {
                println!("  Unsupported wire type");
                break;
            }
        }
    }

    Ok(())
}

fn analyze_nested_message(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut offset = 0;

    println!("  Nested message fields:");
    while offset < data.len() {
        let key = data[offset];
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        offset += 1;

        if wire_type == 2 {
            let (length, length_bytes) = read_varint(data, offset)?;
            offset += length_bytes;

            if offset + length as usize <= data.len() {
                let field_data = &data[offset..offset + length as usize];
                if let Ok(s) = std::str::from_utf8(field_data) {
                    println!("    Field {}: {:?}", field_number, s);
                }
                offset += length as usize;
            }
        }
    }

    Ok(())
}

fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), Box<dyn std::error::Error>> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;

    loop {
        if pos >= data.len() {
            return Err("Truncated varint".into());
        }

        let byte = data[pos];
        pos += 1;

        result |= ((byte & 0x7f) as u64) << shift;

        if byte & 0x80 == 0 {
            break;
        }

        shift += 7;
    }

    Ok((result, pos - offset))
}

fn provide_recommendations(data: &[u8], blob_type: &str) {
    match blob_type {
        "assistant-message-nested" => {
            println!("• Field 1 contains a NESTED MESSAGE, not a direct string");
            println!("• The nested message has Field 1 with the actual text content");
            println!("• Current schema incorrectly reads this as a string, causing corruption");
            println!("• Recommendation: Define a nested message type for Field 1");
        }
        "user-message-direct" => {
            println!("• Field 1 contains a direct string (correct)");
            println!("• No schema changes needed for this blob type");
        }
        "tree-or-reference-blob" => {
            println!("• This is an internal Cursor structure (tree/reference blob)");
            println!("• Not a message - should be skipped during conversion");
        }
        "json-message" => {
            println!("• This is a JSON message in Anthropic API format");
            println!("• JSON decoding should handle this correctly");
        }
        _ => {
            println!("• Unknown blob type - manual investigation needed");
        }
    }
}

fn print_usage() {
    println!("Cursor Blob Analyzer");
    println!();
    println!("USAGE:");
    println!("  cargo run --bin cursor_blob_analyzer -- --db <db_path> --blob-id <blob_id>");
    println!("  cargo run --bin cursor_blob_analyzer -- --hex <hex_data>");
    println!();
    println!("EXAMPLES:");
    println!("  # Analyze blob from database");
    println!("  cargo run --bin cursor_blob_analyzer -- --db ~/.cursor/chats/abc/def/store.db --blob-id 123abc");
    println!();
    println!("  # Analyze hex data");
    println!("  cargo run --bin cursor_blob_analyzer -- --hex 0A0B48656C6C6F");
}
