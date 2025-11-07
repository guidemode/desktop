/// SQLite database utilities for Cursor sessions
use super::protobuf::CursorBlob;
use super::types::SessionMetadata;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Open a Cursor database in read-only mode
///
/// This is safe for concurrent access while Cursor is writing due to WAL mode.
pub fn open_cursor_db(db_path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    // Optimize for read performance
    conn.execute_batch("PRAGMA synchronous = NORMAL;")?;

    Ok(conn)
}

/// Get the data version for change detection
///
/// This is a fast O(1) operation that returns an integer counter.
/// Compare across the same connection to detect database changes.
pub fn get_data_version(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("PRAGMA data_version", [], |row| row.get(0))
}

/// Get session metadata from the meta table
pub fn get_session_metadata(conn: &Connection) -> Result<SessionMetadata, Box<dyn std::error::Error>> {
    let meta_hex: String =
        conn.query_row("SELECT value FROM meta WHERE key = '0'", [], |row| {
            row.get(0)
        })?;

    let meta_json = hex::decode(&meta_hex)?;
    let metadata: SessionMetadata = serde_json::from_slice(&meta_json)?;

    Ok(metadata)
}

/// Get all blobs from the blobs table
pub fn get_all_blobs(
    conn: &Connection,
) -> Result<Vec<(String, Vec<u8>)>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id, data FROM blobs ORDER BY rowid")?;

    let blobs = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(blobs)
}

/// Get all blobs decoded as CursorBlob structs (legacy - protobuf only)
///
/// **Deprecated**: Use `get_decoded_messages()` instead for hybrid protobuf/JSON support.
/// This function only decodes protobuf and will fail on JSON messages.
pub fn get_decoded_blobs(
    conn: &Connection,
) -> Result<Vec<(String, CursorBlob)>, Box<dyn std::error::Error>> {
    let blobs = get_all_blobs(conn)?;
    let mut decoded = Vec::new();

    for (id, data) in blobs {
        match CursorBlob::decode_from_bytes(&data) {
            Ok(blob) => decoded.push((id, blob)),
            Err(_e) => {
                // Decode failed - likely a tree/reference blob or JSON message
                // Use get_decoded_messages() for proper hybrid support
                tracing::debug!("Skipping blob {} (use get_decoded_messages for full support)", id);
                // Continue with other blobs
            }
        }
    }

    Ok(decoded)
}

/// Get all blobs decoded as hybrid CursorMessage (supports both protobuf and JSON)
///
/// Returns tuples of (blob_id, raw_data, decoded_message).
/// The raw_data is needed for fallback decoding of user messages where Field 1 is a direct string.
#[allow(clippy::type_complexity)]
pub fn get_decoded_messages(
    conn: &Connection,
) -> Result<Vec<(String, Vec<u8>, super::protobuf::CursorMessage)>, Box<dyn std::error::Error>> {
    let blobs = get_all_blobs(conn)?;
    let mut decoded = Vec::new();
    let total_blobs = blobs.len();

    tracing::info!("Decoding {} blobs from Cursor database", total_blobs);

    for (idx, (id, data)) in blobs.into_iter().enumerate() {
        tracing::debug!("--- Blob {}/{}: {} ---", idx + 1, total_blobs, id);

        match super::protobuf::CursorMessage::decode_from_bytes(&data) {
            Ok(msg) => {
                let msg_type = match &msg {
                    super::protobuf::CursorMessage::Protobuf(_) => "Protobuf",
                    super::protobuf::CursorMessage::Json(_) => "JSON",
                };
                tracing::info!("✓ Successfully decoded blob {} as {} (role: {})", id, msg_type, msg.get_role());
                // Store raw data alongside decoded message for fallback decoding
                decoded.push((id, data, msg));
            }
            Err(_e) => {
                // Decode failed - likely a tree/reference blob (internal Cursor structure)
                // These are expected and not actual messages, so we skip them silently
                tracing::debug!("⊘ Skipping non-message blob {} (tree/reference blob)", id);
                // Continue with other blobs
            }
        }
    }

    tracing::info!(
        "Decode summary: {}/{} blobs successful ({:.1}% success rate)",
        decoded.len(),
        total_blobs,
        (decoded.len() as f64 / total_blobs as f64) * 100.0
    );

    Ok(decoded)
}

/// Get the count of blobs in the database
pub fn get_blob_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT COUNT(*) FROM blobs", [], |row| row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_data_version_tracking() {
        // Create a temporary database
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create and populate database
        {
            let conn =
                Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE blobs (id TEXT PRIMARY KEY, data BLOB);
                 CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);"
            )
            .unwrap();

            conn.execute(
                "INSERT INTO meta VALUES ('0', ?)",
                [hex::encode(
                    r#"{"agentId":"test","latestRootBlobId":"test","name":"Test","mode":"default","createdAt":1234567890,"lastUsedModel":"default"}"#
                )],
            )
            .unwrap();
        }

        // Open read-only and check data version
        let conn = open_cursor_db(&db_path).unwrap();
        let version1 = get_data_version(&conn).unwrap();

        // Version should be consistent when nothing changes
        let version2 = get_data_version(&conn).unwrap();
        assert_eq!(version1, version2);
    }
}
