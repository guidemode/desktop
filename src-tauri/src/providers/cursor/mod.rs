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
pub mod debug;
pub mod protobuf;
pub mod scanner;
pub mod types;
pub mod watcher;

pub use scanner::scan_existing_sessions;
pub use types::CursorSession;

use std::fs;
use std::path::{Path, PathBuf};

/// Scan for Cursor sessions and return project information
///
/// This function:
/// 1. Iterates through hash directories in {base_path}/chats
/// 2. Finds session directories with store.db files
/// 3. Attempts to read session metadata for project names
pub fn scan_projects(home_directory: &str) -> Result<Vec<crate::config::ProjectInfo>, String> {
    let base_path = shellexpand::tilde(home_directory).to_string();
    let chats_dir = Path::new(&base_path).join("chats");

    if !chats_dir.exists() {
        return Err(format!(
            "Cursor chats directory not found: {}",
            chats_dir.display()
        ));
    }

    let mut projects = Vec::new();

    // Iterate through hash directories
    for hash_entry in fs::read_dir(&chats_dir).map_err(|e| e.to_string())? {
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
                        // Convert created_at timestamp to RFC3339
                        let last_modified = chrono::DateTime::from_timestamp_millis(metadata.created_at)
                            .unwrap_or_else(chrono::Utc::now)
                            .to_rfc3339();

                        // Try to find CWD and derive project name
                        let cwd = find_cwd_for_session(&hash, &Path::new(&base_path).join("projects"));
                        let project_name = cwd
                            .as_ref()
                            .and_then(|path| {
                                Path::new(path)
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| metadata.name.clone());

                        projects.push(crate::config::ProjectInfo {
                            name: project_name,
                            path: session_id, // Use session ID as path
                            last_modified,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read metadata for session {}: {:?}",
                            session_id,
                            e
                        );
                        // Add with generic name
                        projects.push(crate::config::ProjectInfo {
                            name: format!("Cursor Session ({})", &session_id[..8]),
                            path: session_id,
                            last_modified: chrono::Utc::now().to_rfc3339(),
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

    // Deduplicate projects by name, keeping the most recent
    let mut unique_projects: std::collections::HashMap<String, crate::config::ProjectInfo> =
        std::collections::HashMap::new();

    for project in projects {
        let should_replace = if let Some(existing) = unique_projects.get(&project.name) {
            // Compare timestamps - keep the most recent
            project.last_modified > existing.last_modified
        } else {
            true
        };

        if should_replace {
            unique_projects.insert(project.name.clone(), project);
        }
    }

    // Convert back to Vec and sort by last_modified (most recent first)
    let mut deduplicated: Vec<_> = unique_projects.into_values().collect();
    deduplicated.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

    Ok(deduplicated)
}

/// Discover all Cursor sessions with their metadata
///
/// Returns a vector of CursorSession structs containing:
/// - Session ID (UUID)
/// - Database path
/// - Session metadata (name, timestamps, model, etc.)
/// - Parent hash
pub fn discover_sessions(base_path: &Path) -> Result<Vec<CursorSession>, Box<dyn std::error::Error>> {
    let chats_dir = base_path.join("chats");

    if !chats_dir.exists() {
        return Err(format!("Cursor chats directory not found: {}", chats_dir.display()).into());
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
                        // Try to find CWD from projects directory
                        let cwd = find_cwd_for_session(&hash, &base_path.join("projects"));

                        sessions.push(CursorSession {
                            session_id,
                            db_path,
                            metadata,
                            hash: hash.clone(),
                            cwd,
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
pub fn get_db_path_for_session(session_id: &str, base_path: &Path) -> Result<PathBuf, String> {
    let sessions = discover_sessions(base_path).map_err(|e| e.to_string())?;

    sessions
        .into_iter()
        .find(|s| s.session_id == session_id)
        .map(|s| s.db_path)
        .ok_or_else(|| format!("Session not found: {}", session_id))
}

/// Find the CWD for a Cursor session by checking the projects directory
///
/// Cursor stores projects in {base_path}/projects with folder names that are
/// the CWD path with the leading / removed and remaining / replaced with -
/// Example: /Users/cliftonc/work/guidemode -> Users-cliftonc-work-guidemode
pub fn find_cwd_for_session(session_hash: &str, projects_dir: &Path) -> Option<String> {
    if !projects_dir.exists() {
        return None;
    }

    // Try to find a matching project by computing hash for each CWD
    if let Ok(entries) = fs::read_dir(projects_dir) {
        for entry in entries.flatten() {
            let project_path = entry.path();
            if !project_path.is_dir() {
                continue;
            }

            // Convert project folder name back to CWD
            // Example: Users-cliftonc-work-guidemode -> /Users/cliftonc/work/guidemode
            if let Some(folder_name) = project_path.file_name().and_then(|n| n.to_str()) {
                let cwd = format!("/{}", folder_name.replace('-', "/"));

                // Compute MD5 hash of CWD to see if it matches the session hash
                let hash = format!("{:x}", md5::compute(cwd.as_bytes()));

                if hash == session_hash {
                    return Some(cwd);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_projects() {
        // This test will only work if Cursor is installed
        // Skip if directory doesn't exist
        let base_path = shellexpand::tilde("~/.cursor").to_string();
        let chats_dir = Path::new(&base_path).join("chats");
        if !chats_dir.exists() {
            return;
        }

        let result = scan_projects("~/.cursor");
        assert!(result.is_ok());
    }

    #[test]
    fn test_discover_sessions() {
        // This test will only work if Cursor is installed
        let base_path = shellexpand::tilde("~/.cursor").to_string();
        let chats_dir = Path::new(&base_path).join("chats");
        if !chats_dir.exists() {
            return;
        }

        let result = discover_sessions(Path::new(&base_path));
        assert!(result.is_ok());
    }
}
