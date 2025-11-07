#![allow(warnings)] // Development tool
/// Cursor Hex Inspector
///
/// Low-level protobuf wire format analyzer for debugging Cursor message blobs.
///
/// Usage:
///   # Inspect hex data directly
///   cargo run --bin cursor_hex_inspector -- <hex_data>
///
///   # Inspect a blob from database
///   cargo run --bin cursor_hex_inspector -- --db <path> --blob-id <id>
///
/// This tool decodes protobuf wire format and displays:
/// - Field numbers and wire types
/// - Field lengths and values
/// - Nested message structures
/// - Raw hex with annotations

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
        // Load from database
        if args.len() < 5 || args[3] != "--blob-id" {
            eprintln!("Error: --db requires --blob-id");
            print_usage();
            std::process::exit(1);
        }
        let db_path = PathBuf::from(&args[2]);
        let blob_id = &args[4];
        inspect_blob_from_db(&db_path, blob_id)
    } else {
        // Inspect hex data directly
        let hex_data = &args[1];
        inspect_hex(hex_data)
    };

    if let Err(e) = result {
        eprintln!("Error: {:?}", e);
        std::process::exit(1);
    }
}

fn inspect_hex(hex_data: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Cursor Hex Inspector ===\n");

    let data = hex::decode(hex_data)?;

    println!("Total bytes: {}\n", data.len());
    println!("Raw hex:\n{}\n", hex::encode(&data));

    analyze_protobuf_wire_format(&data)?;

    Ok(())
}

fn inspect_blob_from_db(
    db_path: &std::path::Path,
    blob_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use rusqlite::Connection;

    println!("=== Cursor Hex Inspector ===\n");
    println!("Database: {}", db_path.display());
    println!("Blob ID: {}\n", blob_id);

    let conn = Connection::open(db_path)?;
    let data: Vec<u8> = conn.query_row(
        "SELECT data FROM blobs WHERE id = ?",
        [blob_id],
        |row| row.get(0),
    )?;

    println!("Total bytes: {}\n", data.len());
    println!("Raw hex:\n{}\n", hex::encode(&data));

    analyze_protobuf_wire_format(&data)?;

    Ok(())
}

/// Analyze protobuf wire format
///
/// Protobuf wire format:
/// - Each field starts with a varint key: (field_number << 3) | wire_type
/// - Wire types:
///   0 = Varint
///   1 = 64-bit
///   2 = Length-delimited (strings, bytes, embedded messages)
///   3 = Start group (deprecated)
///   4 = End group (deprecated)
///   5 = 32-bit
fn analyze_protobuf_wire_format(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Protobuf Wire Format Analysis ===\n");

    let mut offset = 0;
    let mut field_num = 0;

    while offset < data.len() {
        // Read varint key
        let (key, key_len) = read_varint(data, offset)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;

        field_num += 1;

        println!("Field #{} @ offset {}", field_num, offset);
        println!("  Key: 0x{:02x} (field={}, wire_type={})",
                 key, field_number, wire_type);

        offset += key_len;

        match wire_type {
            0 => {
                // Varint
                let (value, value_len) = read_varint(data, offset)?;
                println!("  Type: Varint");
                println!("  Value: {} (0x{:x})", value, value);
                println!("  Bytes: {}", hex::encode(&data[offset..offset + value_len]));
                offset += value_len;
            }
            1 => {
                // 64-bit
                if offset + 8 > data.len() {
                    return Err("Truncated 64-bit field".into());
                }
                println!("  Type: 64-bit");
                println!("  Bytes: {}", hex::encode(&data[offset..offset + 8]));
                offset += 8;
            }
            2 => {
                // Length-delimited (string, bytes, or embedded message)
                let (length, length_len) = read_varint(data, offset)?;
                offset += length_len;

                if offset + length as usize > data.len() {
                    return Err(format!("Truncated length-delimited field: need {} bytes, have {}",
                                      length, data.len() - offset).into());
                }

                let field_data = &data[offset..offset + length as usize];

                println!("  Type: Length-delimited");
                println!("  Length: {} bytes", length);

                // Try to interpret as string
                if let Ok(s) = std::str::from_utf8(field_data) {
                    println!("  As string: {:?}", s);
                } else {
                    println!("  As string: <invalid UTF-8>");
                }

                // Show hex
                if length <= 100 {
                    println!("  Hex: {}", hex::encode(field_data));
                } else {
                    println!("  Hex (first 100 bytes): {}", hex::encode(&field_data[..100]));
                }

                // Try to interpret as nested message
                if field_data.len() > 0 && field_data[0] < 0x20 {
                    println!("  Possible nested message (starts with 0x{:02x}):", field_data[0]);
                    match analyze_nested_message(field_data, 2) {
                        Ok(_) => {},
                        Err(e) => println!("    Failed to parse as nested: {}", e),
                    }
                }

                offset += length as usize;
            }
            5 => {
                // 32-bit
                if offset + 4 > data.len() {
                    return Err("Truncated 32-bit field".into());
                }
                println!("  Type: 32-bit");
                println!("  Bytes: {}", hex::encode(&data[offset..offset + 4]));
                offset += 4;
            }
            _ => {
                return Err(format!("Unknown wire type: {}", wire_type).into());
            }
        }

        println!();
    }

    Ok(())
}

/// Analyze nested message with indentation
fn analyze_nested_message(data: &[u8], indent: usize) -> Result<(), Box<dyn std::error::Error>> {
    let prefix = "  ".repeat(indent);
    let mut offset = 0;

    while offset < data.len() {
        // Read varint key
        let (key, key_len) = read_varint(data, offset)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;

        print!("{}Field {} ", prefix, field_number);
        offset += key_len;

        match wire_type {
            0 => {
                let (value, value_len) = read_varint(data, offset)?;
                println!("(varint): {}", value);
                offset += value_len;
            }
            2 => {
                let (length, length_len) = read_varint(data, offset)?;
                offset += length_len;

                if offset + length as usize > data.len() {
                    return Err("Truncated nested field".into());
                }

                let field_data = &data[offset..offset + length as usize];

                if let Ok(s) = std::str::from_utf8(field_data) {
                    println!("(string): {:?}", s);
                } else {
                    println!("(bytes, {} bytes)", length);
                }

                offset += length as usize;
            }
            _ => {
                println!("(wire_type {})", wire_type);
                break;
            }
        }
    }

    Ok(())
}

/// Read a protobuf varint
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
        if shift >= 64 {
            return Err("Varint too long".into());
        }
    }

    Ok((result, pos - offset))
}

fn print_usage() {
    println!("Cursor Hex Inspector");
    println!();
    println!("USAGE:");
    println!("  cargo run --bin cursor_hex_inspector -- <hex_data>");
    println!("  cargo run --bin cursor_hex_inspector -- --db <db_path> --blob-id <blob_id>");
    println!();
    println!("EXAMPLES:");
    println!("  # Inspect hex data directly");
    println!("  cargo run --bin cursor_hex_inspector -- 0A0B48656C6C6F20776F726C64");
    println!();
    println!("  # Inspect blob from database");
    println!("  cargo run --bin cursor_hex_inspector -- --db ~/.cursor/chats/abc/def/store.db --blob-id 123abc");
}
