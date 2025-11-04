/// Cursor CLI provider module
///
/// Handles Cursor session detection, protobuf decoding, and conversion to canonical format.
///
/// Architecture:
/// - SQLite databases (one per session) at ~/.cursor/chats/{hash}/{uuid}/store.db
/// - Protocol Buffer encoded messages in blobs table
/// - Content-addressable storage with SHA-256 blob IDs
/// - WAL mode for safe concurrent access

pub mod converter;
pub mod db;
pub mod protobuf;
pub mod scanner;
pub mod types;

pub use scanner::{scan_existing_sessions, write_canonical_file, ScanResult};
pub use types::{CursorSession, ProjectInfo, SessionMetadata};

use std::fs;
use std::path::{Path, PathBuf};

const CURSOR_CHATS_DIR: &str = "~/.cursor/chats";

/// Scan for Cursor sessions and return project information
///
/// This function:
/// 1. Iterates through hash directories in ~/.cursor/chats
/// 2. Finds session directories with store.db files
/// 3. Attempts to read session metadata for project names
pub fn scan_projects(_home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    let chats_path = shellexpand::tilde(CURSOR_CHATS_DIR).to_string();
    let chats_dir = Path::new(&chats_path);

    if !chats_dir.exists() {
        return Err(format!(
            "Cursor chats directory not found: {}",
            chats_path
        ));
    }

    let mut projects = Vec::new();

    // Iterate through hash directories
    for hash_entry in fs::read_dir(chats_dir).map_err(|e| e.to_string())? {
        let hash_entry = hash_entry.map_err(|e| e.to_string())?;
        let hash_path = hash_entry.path();

        if !hash_path.is_dir() {
            continue;
        }

        let hash = hash_entry
            .file_name()
            .to_string_lossy()
            .to_string();

        // Iterate through session directories
        for session_entry in fs::read_dir(&hash_path).map_err(|e| e.to_string())? {
            let session_entry = session_entry.map_err(|e| e.to_string())?;
            let session_path = session_entry.path();

            if !session_path.is_dir() {
                continue;
            }

            let db_path = session_path.join("store.db");
            if !db_path.exists() {
                continue;
            }

            let session_id = session_entry.file_name().to_string_lossy().to_string();

            // Try to get session name from metadata
            match db::open_cursor_db(&db_path) {
                Ok(conn) => match db::get_session_metadata(&conn) {
                    Ok(metadata) => {
                        projects.push(ProjectInfo {
                            name: metadata.name,
                            path: session_id, // Use session ID as path
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read metadata for session {}: {:?}",
                            session_id,
                            e
                        );
                        // Add with generic name
                        projects.push(ProjectInfo {
                            name: format!("Cursor Session ({})", &session_id[..8]),
                            path: session_id,
                        });
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to open database for session {}: {:?}",
                        session_id,
                        e
                    );
                }
            }
        }
    }

    Ok(projects)
}

/// Discover all Cursor sessions with their metadata
///
/// Returns a vector of CursorSession structs containing:
/// - Session ID (UUID)
/// - Database path
/// - Session metadata (name, timestamps, model, etc.)
/// - Parent hash
pub fn discover_sessions() -> Result<Vec<CursorSession>, Box<dyn std::error::Error>> {
    let chats_path = shellexpand::tilde(CURSOR_CHATS_DIR).to_string();
    let chats_dir = Path::new(&chats_path);

    if !chats_dir.exists() {
        return Err(format!("Cursor chats directory not found: {}", chats_path).into());
    }

    let mut sessions = Vec::new();

    // Iterate through hash directories
    for hash_entry in fs::read_dir(chats_dir)? {
        let hash_entry = hash_entry?;
        let hash_path = hash_entry.path();

        if !hash_path.is_dir() {
            continue;
        }

        let hash = hash_entry.file_name().to_string_lossy().to_string();

        // Iterate through session directories
        for session_entry in fs::read_dir(&hash_path)? {
            let session_entry = session_entry?;
            let session_path = session_entry.path();

            if !session_path.is_dir() {
                continue;
            }

            let db_path = session_path.join("store.db");
            if !db_path.exists() {
                continue;
            }

            let session_id = session_entry.file_name().to_string_lossy().to_string();

            // Try to get session metadata
            match db::open_cursor_db(&db_path) {
                Ok(conn) => match db::get_session_metadata(&conn) {
                    Ok(metadata) => {
                        sessions.push(CursorSession {
                            session_id,
                            db_path,
                            metadata,
                            hash: hash.clone(),
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read metadata for session {}: {:?}",
                            session_id,
                            e
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to open database for session {}: {:?}",
                        session_id,
                        e
                    );
                }
            }
        }
    }

    Ok(sessions)
}

/// Get the Cursor database path from a session ID
///
/// Note: This requires scanning to find which hash directory contains the session
pub fn get_db_path_for_session(session_id: &str) -> Result<PathBuf, String> {
    let sessions = discover_sessions().map_err(|e| e.to_string())?;

    sessions
        .into_iter()
        .find(|s| s.session_id == session_id)
        .map(|s| s.db_path)
        .ok_or_else(|| format!("Session not found: {}", session_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_projects() {
        // This test will only work if Cursor is installed
        // Skip if directory doesn't exist
        let chats_path = shellexpand::tilde(CURSOR_CHATS_DIR).to_string();
        if !Path::new(&chats_path).exists() {
            return;
        }

        let result = scan_projects("");
        assert!(result.is_ok());
    }

    #[test]
    fn test_discover_sessions() {
        // This test will only work if Cursor is installed
        let chats_path = shellexpand::tilde(CURSOR_CHATS_DIR).to_string();
        if !Path::new(&chats_path).exists() {
            return;
        }

        let result = discover_sessions();
        assert!(result.is_ok());
    }
}
