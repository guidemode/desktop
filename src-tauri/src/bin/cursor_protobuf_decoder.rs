#![allow(warnings)] // Development tool
/// Cursor Protobuf Decoder
/// Attempts to decode the protobuf structure manually to understand the schema

use rusqlite::{Connection, OpenFlags};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = shellexpand::tilde(
        "~/.cursor/chats/0d265392dfc786bc1af0df28bb21fea3/a562b4a7-31d2-45d6-9141-2be5c4edf3ef/store.db"
    );

    let conn = Connection::open_with_flags(
        Path::new(db_path.as_ref()),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;

    println!("=== Cursor Protobuf Structure Analysis ===\n");

    // Get all blobs
    let mut stmt = conn.prepare("SELECT id, data FROM blobs")?;
    let blobs: Vec<(String, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    println!("Analyzing {} blobs\n", blobs.len());

    for (i, (id, data)) in blobs.iter().enumerate() {
        println!("━━━ Blob #{} ━━━", i + 1);
        println!("ID: {}", &id[..16]);
        println!("Size: {} bytes", data.len());

        // Manual protobuf field parsing
        decode_protobuf_fields(data);

        println!();
    }

    Ok(())
}

fn decode_protobuf_fields(data: &[u8]) {
    let mut pos = 0;

    while pos < data.len() {
        // Read varint tag
        let (tag, tag_size) = match read_varint(&data[pos..]) {
            Some((v, s)) => (v, s),
            None => break,
        };

        pos += tag_size;

        let field_number = tag >> 3;
        let wire_type = tag & 0x7;

        print!("  Field {}: ", field_number);

        match wire_type {
            0 => {
                // Varint
                if let Some((value, size)) = read_varint(&data[pos..]) {
                    println!("varint = {}", value);
                    pos += size;
                } else {
                    println!("ERROR reading varint");
                    break;
                }
            }
            1 => {
                // 64-bit
                if pos + 8 <= data.len() {
                    println!("64-bit = {:?}", &data[pos..pos + 8]);
                    pos += 8;
                } else {
                    println!("ERROR: not enough bytes for 64-bit");
                    break;
                }
            }
            2 => {
                // Length-delimited (string, bytes, embedded message)
                if let Some((length, size)) = read_varint(&data[pos..]) {
                    pos += size;
                    let length = length as usize;

                    if pos + length <= data.len() {
                        let field_data = &data[pos..pos + length];

                        // Try to interpret as string
                        if let Ok(s) = std::str::from_utf8(field_data) {
                            if s.chars().all(|c| c.is_ascii_graphic() || c.is_whitespace()) {
                                println!("string = \"{}\"", s);
                            } else {
                                println!("bytes ({} bytes) = {}", length, hex::encode(&field_data[..std::cmp::min(32, length)]));
                            }
                        } else {
                            // Might be an embedded message or binary data
                            println!("bytes ({} bytes, might be submessage) = {}", length, hex::encode(&field_data[..std::cmp::min(32, length)]));

                            // Try to parse as nested message
                            if length > 2 {
                                println!("    -> Attempting to decode as nested message:");
                                decode_protobuf_fields(field_data);
                            }
                        }

                        pos += length;
                    } else {
                        println!("ERROR: length {} exceeds remaining data", length);
                        break;
                    }
                } else {
                    println!("ERROR reading length");
                    break;
                }
            }
            5 => {
                // 32-bit
                if pos + 4 <= data.len() {
                    println!("32-bit = {:?}", &data[pos..pos + 4]);
                    pos += 4;
                } else {
                    println!("ERROR: not enough bytes for 32-bit");
                    break;
                }
            }
            _ => {
                println!("UNKNOWN wire type {}", wire_type);
                break;
            }
        }
    }
}

fn read_varint(data: &[u8]) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0;

    for (i, &byte) in data.iter().enumerate() {
        if i > 9 {
            return None; // Varint too long
        }

        result |= ((byte & 0x7F) as u64) << shift;

        if byte & 0x80 == 0 {
            return Some((result, i + 1));
        }

        shift += 7;
    }

    None
}
