use std::fs;
use std::path::PathBuf;

/// Extract CWD from canonical JSONL content
/// Returns the session start directory (not mid-session directory changes)
/// Scans first 50 lines, skipping snapshot messages to find the initial CWD
pub fn extract_cwd_from_canonical_content(content: &str) -> Option<String> {
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return None;
    }

    // Skip snapshots and meta messages to get CWD from first real message
    // This ensures we get the session start directory, not where the session navigated to later
    for line in lines.iter().take(50) {
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
            // Skip file-history-snapshot messages (meta data, not session messages)
            if let Some(msg_type) = entry.get("type").and_then(|v| v.as_str()) {
                if msg_type == "file-history-snapshot" {
                    continue;
                }
            }

            // Return first CWD found after skipping snapshots
            // This will be the session start directory
            if let Some(cwd) = entry.get("cwd").and_then(|v| v.as_str()) {
                return Some(cwd.to_string());
            }
        }
    }

    None
}

/// Sanitize project name for filesystem safety
/// Replaces spaces, slashes, and special characters with safe alternatives
pub fn sanitize_project_name(name: &str) -> String {
    name.replace(['/', '\\', ' ', ':', '*', '?', '"', '<', '>', '|'], "-")
        .trim_matches('-')
        .to_string()
}

/// Get canonical path for a session file, organized by project
/// Path format: ~/.guideai/sessions/{provider}/{project}/{session_id}.jsonl
///
/// If CWD is provided, attempts to extract project name using project_metadata.
/// Falls back to "unknown" if CWD is None or project extraction fails.
pub fn get_canonical_path(
    provider_id: &str,
    cwd: Option<&str>,
    session_id: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    // Get base sessions directory
    let sessions_base = dirs::home_dir()
        .ok_or("Failed to get home directory")?
        .join(".guideai")
        .join("sessions")
        .join(provider_id);

    // Determine project name from CWD
    let project_name = if let Some(cwd_path) = cwd {
        // Try to extract project metadata from CWD
        match crate::project_metadata::extract_project_metadata(cwd_path) {
            Ok(metadata) => sanitize_project_name(&metadata.project_name),
            Err(_) => {
                // CWD provided but couldn't extract project name - reject this session
                return Err("Cannot determine project name from CWD - session will not be cached".into());
            }
        }
    } else {
        // No CWD provided - reject this session
        return Err("No CWD available - session will not be cached until CWD is found".into());
    };

    // Create full path with project subdirectory
    let project_dir = sessions_base.join(&project_name);

    // Ensure parent directories exist
    fs::create_dir_all(&project_dir)?;

    // Create session file path
    let session_path = project_dir.join(format!("{}.jsonl", session_id));

    Ok(session_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_cwd_from_canonical_content() {
        // Test with CWD in first line
        let content = r#"{"timestamp":"2024-01-01T00:00:00Z","cwd":"/home/user/project","message":"test"}
{"timestamp":"2024-01-01T00:00:01Z","message":"test2"}"#;
        assert_eq!(
            extract_cwd_from_canonical_content(content),
            Some("/home/user/project".to_string())
        );

        // Test with CWD not in first line
        let content = r#"{"timestamp":"2024-01-01T00:00:00Z","message":"test"}
{"timestamp":"2024-01-01T00:00:01Z","cwd":"/home/user/another","message":"test2"}"#;
        assert_eq!(
            extract_cwd_from_canonical_content(content),
            Some("/home/user/another".to_string())
        );

        // Test with snapshot message (should skip and get next CWD)
        let content = r#"{"type":"file-history-snapshot","cwd":"/wrong/directory"}
{"type":"user","cwd":"/home/user/project","message":"test"}"#;
        assert_eq!(
            extract_cwd_from_canonical_content(content),
            Some("/home/user/project".to_string())
        );

        // Test with session start directory vs mid-session navigation
        // Should return the first non-snapshot CWD (session start), not later CWD changes
        let content = r#"{"type":"user","cwd":"/home/user/project","message":"start"}
{"type":"assistant","cwd":"/home/user/project/packages/foo","message":"navigated"}"#;
        assert_eq!(
            extract_cwd_from_canonical_content(content),
            Some("/home/user/project".to_string())
        );

        // Test with no CWD
        let content = r#"{"timestamp":"2024-01-01T00:00:00Z","message":"test"}
{"timestamp":"2024-01-01T00:00:01Z","message":"test2"}"#;
        assert_eq!(extract_cwd_from_canonical_content(content), None);

        // Test with empty content
        assert_eq!(extract_cwd_from_canonical_content(""), None);
    }

    #[test]
    fn test_sanitize_project_name() {
        assert_eq!(sanitize_project_name("my-project"), "my-project");
        assert_eq!(sanitize_project_name("my project"), "my-project");
        assert_eq!(sanitize_project_name("my/project"), "my-project");
        assert_eq!(sanitize_project_name("my\\project"), "my-project");
        assert_eq!(sanitize_project_name("my:project"), "my-project");
        assert_eq!(sanitize_project_name("my*project?"), "my-project");
        assert_eq!(
            sanitize_project_name("my project/with spaces"),
            "my-project-with-spaces"
        );
        assert_eq!(sanitize_project_name("---test---"), "test");
    }

    #[test]
    fn test_get_canonical_path_structure() {
        // Test that path returns error when CWD can't be extracted
        let result = get_canonical_path("test-provider", Some("/tmp/test"), "session123");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot determine project name"));
    }

    #[test]
    fn test_get_canonical_path_without_cwd() {
        // Should return error when no CWD provided
        let result = get_canonical_path("test-provider", None, "session456");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No CWD available"));
    }
}
