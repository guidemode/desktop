use super::sort_projects_by_modified;
use crate::config::ProjectInfo;
use crate::providers::gemini_parser::GeminiSession;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use shellexpand::tilde;
use std::fs;
use std::path::{Path, PathBuf};

pub fn scan_projects(home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    use super::gemini_registry::GeminiProjectRegistry;

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

    // Load existing registry
    let mut registry = GeminiProjectRegistry::load()
        .map_err(|e| format!("Failed to load project registry: {}", e))?;

    let entries =
        fs::read_dir(&tmp_path).map_err(|e| format!("Failed to read tmp directory: {}", e))?;

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

        // Try to get project info from registry first
        let project_name = if let Some(entry) = registry.get_project(hash) {
            // Found in registry - use cached values
            entry.name.clone()
        } else {
            // Not in registry - parse session files to extract CWD
            match resolve_project_info(&path, hash) {
                Ok((name, cwd)) => {
                    // Successfully extracted CWD - add to registry
                    registry.update_project(hash.to_string(), cwd.clone(), name.clone());
                    name
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Could not resolve project info for {}: {}",
                        hash, e
                    );
                    // Fallback to shortened hash for name
                    // Don't add to registry since we don't have real CWD
                    format!("gemini-{}", &hash[..8])
                }
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
                path: hash.to_string(), // CRITICAL: Store hash in path field (not CWD)
                last_modified: modified.to_rfc3339(),
            },
        ));
    }

    // Save updated registry back to disk
    if let Err(e) = registry.save() {
        eprintln!("Warning: Failed to save project registry: {}", e);
        // Continue even if save fails - not fatal
    }

    Ok(sort_projects_by_modified(projects))
}

/// Resolve project name and CWD from hash by examining session files
/// Returns (project_name, cwd_path)
fn resolve_project_info(project_path: &Path, hash: &str) -> Result<(String, String), String> {
    let chats_path = project_path.join("chats");

    if !chats_path.exists() {
        return Err("No chats directory found".to_string());
    }

    // Try to read the first session file
    let entries =
        fs::read_dir(&chats_path).map_err(|e| format!("Failed to read chats directory: {}", e))?;

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
                if let Some(cwd) = infer_cwd_from_session(&session, hash) {
                    let project_name = get_project_name_from_path(&cwd)?;
                    return Ok((project_name, cwd));
                }
            }
        }
    }

    Err("Could not determine project info from sessions".to_string())
}

/// Get project name from working directory path
fn get_project_name_from_path(workdir: &str) -> Result<String, String> {
    Path::new(workdir)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Could not extract project name from path".to_string())
}

/// Extract all candidate file paths from message content
/// Looks for absolute paths, especially after '---' delimiters (common in tool output)
pub fn extract_candidate_paths_from_content(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for line in lines {
        // Look for absolute paths (Unix and Windows)
        if line.contains("/Users/") || line.contains("/home/") || line.contains("C:\\") {
            // Prefer paths after '---' delimiter (common in tool output)
            let search_text = if let Some(delimiter_pos) = line.find("---") {
                &line[delimiter_pos + 3..]
            } else {
                line
            };

            // Extract all absolute paths from the line
            let parts: Vec<&str> = search_text.split_whitespace().collect();
            for part in parts {
                // Unix paths or Windows paths
                if part.starts_with('/')
                    || (part.len() > 3
                        && part.chars().nth(1) == Some(':')
                        && part.chars().nth(2) == Some('\\'))
                {
                    paths.push(part.to_string());
                }
            }
        }
    }

    paths
}

/// Try progressively shorter paths until we find one matching the hash
pub fn find_matching_path(full_path: &str, expected_hash: &str) -> Option<String> {
    let path_buf = Path::new(full_path);
    let mut current_path = path_buf;

    // Try the full path first, then progressively remove the last segment
    loop {
        if let Some(path_str) = current_path.to_str() {
            // Skip root and empty paths
            if !path_str.is_empty() && path_str != "/" && path_str != "\\" {
                // Test if this path's hash matches
                if verify_hash(path_str, expected_hash) {
                    return Some(path_str.to_string());
                }
            }
        }

        // Move up to parent directory
        match current_path.parent() {
            Some(parent) if parent != current_path => {
                current_path = parent;
            }
            _ => break, // No more parents or reached root
        }
    }

    None
}

/// Verify that SHA256(workdir) == hash
pub fn verify_hash(workdir: &str, expected_hash: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(workdir.as_bytes());
    let result = hasher.finalize();
    let computed_hash = hex::encode(result);
    computed_hash == expected_hash
}

/// Infer working directory from Gemini session messages
/// This is the canonical CWD extraction function used by both watcher and scanner
/// Priority order: 1) Tool call arguments (most reliable), 2) Extended Thinking, 3) Message content
pub fn infer_cwd_from_session(session: &GeminiSession, project_hash: &str) -> Option<String> {
    for message in &session.messages {
        // PRIORITY 1: Tool call arguments (most reliable - structured data with explicit paths)
        if let Some(ref tool_calls) = message.tool_calls {
            for tool_call in tool_calls {
                if let Some(ref args) = tool_call.args {
                    let paths = extract_paths_from_tool_args(args);
                    for path in paths {
                        if let Some(matching_path) = find_matching_path(&path, project_hash) {
                            return Some(matching_path);
                        }
                    }
                }
            }
        }

        // PRIORITY 2: Extended Thinking (works well - paths often appear in thought descriptions)
        if let Some(ref thoughts) = message.thoughts {
            for thought in thoughts {
                let thought_paths = extract_candidate_paths_from_content(&thought.description);

                for base_path in thought_paths {
                    if let Some(matching_path) = find_matching_path(&base_path, project_hash) {
                        return Some(matching_path);
                    }
                }
            }
        }

        // PRIORITY 3: Message content (fallback - regex-based extraction)
        let candidate_paths = extract_candidate_paths_from_content(&message.content);

        for base_path in candidate_paths {
            if let Some(matching_path) = find_matching_path(&base_path, project_hash) {
                return Some(matching_path);
            }
        }
    }
    None
}

/// Extract file paths from tool call arguments
/// Handles various Gemini tool argument patterns (absolute_path, paths array, path field)
fn extract_paths_from_tool_args(args: &serde_json::Value) -> Vec<String> {
    let mut paths = Vec::new();

    // Check for absolute_path field (read_file, write_file, edit, etc.)
    if let Some(path) = args.get("absolute_path").and_then(|v| v.as_str()) {
        paths.push(path.to_string());
    }

    // Check for paths array (read_many_files, glob results)
    if let Some(paths_arr) = args.get("paths").and_then(|v| v.as_array()) {
        for path_val in paths_arr {
            if let Some(path_str) = path_val.as_str() {
                paths.push(path_str.to_string());
            }
        }
    }

    // Check for path field (some tools use this variant)
    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        paths.push(path.to_string());
    }

    paths
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

    // Tests for CWD extraction are now in tests/gemini_cwd_extraction.rs
}
