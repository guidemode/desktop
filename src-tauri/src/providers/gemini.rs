use super::sort_projects_by_modified;
use crate::config::ProjectInfo;
use crate::providers::gemini_parser::GeminiSession;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use shellexpand::tilde;
use std::fs;
use std::path::{Path, PathBuf};

pub fn scan_projects(home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    let expanded = tilde(home_directory);
    let base_path = PathBuf::from(expanded.into_owned());

    if !base_path.exists() {
        return Err(format!(
            "Gemini home directory not found: {}",
            base_path.display()
        ));
    }

    let tmp_path = base_path.join("tmp");
    if !tmp_path.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&tmp_path)
        .map_err(|e| format!("Failed to read tmp directory: {}", e))?;

    let mut projects = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip the 'bin' directory
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name == "bin" {
                continue;
            }
        }

        if !path.is_dir() {
            continue;
        }

        let Some(hash) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // Try to resolve the project name from the hash
        let project_name = match resolve_project_name(&path, hash) {
            Ok(name) => name,
            Err(e) => {
                eprintln!("Warning: Could not resolve project name for {}: {}", hash, e);
                // Fallback to shortened hash
                format!("gemini-{}", &hash[..8])
            }
        };

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        let Ok(modified) = metadata.modified() else {
            continue;
        };

        let modified: DateTime<Utc> = modified.into();
        projects.push((
            modified,
            ProjectInfo {
                name: project_name,
                path: path.to_string_lossy().to_string(),
                last_modified: modified.to_rfc3339(),
            },
        ));
    }

    Ok(sort_projects_by_modified(projects))
}

/// Resolve project name from hash by examining session files
fn resolve_project_name(project_path: &Path, hash: &str) -> Result<String, String> {
    let chats_path = project_path.join("chats");

    if !chats_path.exists() {
        return Err("No chats directory found".to_string());
    }

    // Try to read the first session file
    let entries = fs::read_dir(&chats_path)
        .map_err(|e| format!("Failed to read chats directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process session JSON files
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if !filename.starts_with("session-") || !filename.ends_with(".json") {
                continue;
            }
        } else {
            continue;
        }

        // Try to parse the session and extract working directory
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(session) = GeminiSession::from_json(&content) {
                // Verify the hash matches
                if session.project_hash != hash {
                    continue;
                }

                // Try to extract working directory from message content
                if let Some(workdir) = extract_working_dir_from_messages(&session) {
                    // Verify hash
                    if verify_hash(&workdir, hash) {
                        return get_project_name_from_path(&workdir);
                    }
                }
            }
        }
    }

    Err("Could not determine project name from sessions".to_string())
}

/// Extract working directory from Gemini message content
fn extract_working_dir_from_messages(session: &GeminiSession) -> Option<String> {
    // Gemini often mentions file paths in its responses
    // Look for patterns like "apps/desktop/package.json" or "packages/..."

    for message in &session.messages {
        if message.message_type == "gemini" {
            // Look for common project structure indicators
            let content = &message.content;

            // Try to find file paths that indicate project structure
            if let Some(workdir) = infer_workdir_from_content(content) {
                return Some(workdir);
            }
        }
    }

    None
}

/// Infer working directory from message content containing file paths
fn infer_workdir_from_content(content: &str) -> Option<String> {
    // Look for absolute paths in the content
    // Common patterns: /Users/username/path/to/project/file
    //                  /home/username/path/to/project/file

    let lines: Vec<&str> = content.lines().collect();

    for line in lines {
        // Look for absolute paths
        if line.contains("/Users/") || line.contains("/home/") {
            // Extract potential paths
            if let Some(path) = extract_base_path_from_line(line) {
                return Some(path);
            }
        }
    }

    None
}

/// Extract base project path from a line containing file paths
fn extract_base_path_from_line(line: &str) -> Option<String> {
    // Look for patterns like "/Users/cliftonc/work/guideai/apps/desktop/..."
    // and extract "/Users/cliftonc/work/guideai"

    // Find path-like segments
    let parts: Vec<&str> = line.split_whitespace().collect();

    for part in parts {
        if part.starts_with('/') && (part.contains("/work/") || part.contains("/projects/")) {
            // Try to extract up to project root
            // Heuristic: take path segments until we hit common subdirs
            let segments: Vec<&str> = part.split('/').collect();

            for (i, segment) in segments.iter().enumerate() {
                // Common project subdirectories
                if ["apps", "packages", "src", "lib", "dist", "node_modules"]
                    .contains(segment)
                {
                    // Take everything before this
                    let base_path = segments[..i].join("/");
                    if !base_path.is_empty() {
                        return Some(base_path);
                    }
                }
            }
        }
    }

    None
}

/// Verify that SHA256(workdir) == hash
fn verify_hash(workdir: &str, expected_hash: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(workdir.as_bytes());
    let result = hasher.finalize();
    let computed_hash = hex::encode(result);
    computed_hash == expected_hash
}

/// Get project name from working directory path
fn get_project_name_from_path(workdir: &str) -> Result<String, String> {
    Path::new(workdir)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Could not extract project name from path".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_hash() {
        let workdir = "/Users/cliftonc/work/guideai";
        let expected = "7e95bdea1c91b994ca74439a92c90b82767abc9c0b8566e20ab60b2a797fc332";
        assert!(verify_hash(workdir, expected));
    }

    #[test]
    fn test_get_project_name() {
        let workdir = "/Users/cliftonc/work/guideai";
        let name = get_project_name_from_path(workdir).unwrap();
        assert_eq!(name, "guideai");
    }

    #[test]
    fn test_extract_base_path() {
        let line = "in `apps/desktop/package.json` at /Users/cliftonc/work/guideai/apps/desktop";
        let path = extract_base_path_from_line(line);
        assert_eq!(path, Some("/Users/cliftonc/work/guideai".to_string()));
    }
}
