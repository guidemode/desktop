/// Cursor Database Analysis Tool
/// Analyzes Cursor's SQLite databases to reverse engineer the protobuf schema

use rusqlite::{Connection, OpenFlags};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = shellexpand::tilde(
        "~/.cursor/chats/0d265392dfc786bc1af0df28bb21fea3/a562b4a7-31d2-45d6-9141-2be5c4edf3ef/store.db"
    );

    println!("=== Cursor Database Analysis ===\n");
    println!("Database: {}\n", db_path);

    let conn = Connection::open_with_flags(
        Path::new(db_path.as_ref()),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;

    // Part 1: Analyze metadata table
    println!("--- Metadata Table ---");
    let meta_hex: String = conn.query_row(
        "SELECT value FROM meta WHERE key = '0'",
        [],
        |row| row.get(0),
    )?;

    println!("Metadata (hex): {}", &meta_hex[..std::cmp::min(100, meta_hex.len())]);

    let meta_bytes = hex::decode(&meta_hex)?;
    let meta_str = String::from_utf8_lossy(&meta_bytes);
    println!("Metadata (decoded): {}\n", meta_str);

    // Part 2: Analyze blobs table
    println!("--- Blobs Table ---");
    let blob_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM blobs",
        [],
        |row| row.get(0),
    )?;
    println!("Total blobs: {}\n", blob_count);

    // Get sample blobs
    let mut stmt = conn.prepare("SELECT id, length(data), data FROM blobs LIMIT 5")?;
    let blobs = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Vec<u8>>(2)?,
        ))
    })?;

    for (i, blob_result) in blobs.enumerate() {
        let (id, len, data) = blob_result?;
        println!("Blob #{}", i + 1);
        println!("  ID: {}", id);
        println!("  Size: {} bytes", len);
        println!("  First 16 bytes (hex): {}", hex::encode(&data[..std::cmp::min(16, data.len())]));

        // Try to identify protobuf field markers
        println!("  Raw bytes sample: {:?}", &data[..std::cmp::min(32, data.len())]);

        // Look for string patterns (protobuf strings are length-prefixed)
        if let Some(text) = extract_visible_text(&data) {
            println!("  Visible text: {}", text);
        }

        println!();
    }

    // Part 3: Analyze database schema
    println!("--- Database Schema ---");
    let mut stmt = conn.prepare(
        "SELECT sql FROM sqlite_master WHERE type='table' ORDER BY name"
    )?;

    let tables = stmt.query_map([], |row| row.get::<_, String>(0))?;

    for table_sql in tables {
        println!("{}\n", table_sql?);
    }

    Ok(())
}

fn extract_visible_text(data: &[u8]) -> Option<String> {
    // Try to extract any human-readable text from the binary data
    let mut text = String::new();
    let mut consecutive_printable = 0;

    for &byte in data {
        if byte >= 32 && byte <= 126 {
            text.push(byte as char);
            consecutive_printable += 1;
        } else if consecutive_printable > 3 {
            text.push(' ');
            consecutive_printable = 0;
        } else {
            if consecutive_printable > 0 {
                // Remove non-text segment
                text.truncate(text.len() - consecutive_printable);
            }
            consecutive_printable = 0;
        }
    }

    if text.trim().len() > 5 {
        Some(text.trim().to_string())
    } else {
        None
    }
}
