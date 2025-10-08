use crate::error::GuideAIError;
use std::path::{Path, PathBuf};

/// Maximum file size for session uploads (100MB)
pub const MAX_SESSION_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// Allowed base directories for file operations
fn get_allowed_directories() -> Result<Vec<PathBuf>, GuideAIError> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| GuideAIError::Validation("Could not find home directory".to_string()))?;

    let mut allowed = vec![
        home_dir.join(".guideai"),        // GuideAI config and logs
        home_dir.join(".claude"),          // Claude Code sessions
        home_dir.join(".codex"),           // Codex sessions
    ];

    // OpenCode path (platform-specific)
    #[cfg(target_os = "macos")]
    allowed.push(home_dir.join(".local/share/opencode"));

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
            allowed.push(PathBuf::from(xdg_data).join("opencode"));
        } else {
            allowed.push(home_dir.join(".local/share/opencode"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = dirs::data_local_dir() {
            allowed.push(local_app_data.join("opencode"));
        }
    }

    Ok(allowed)
}

/// Validate that a file path is safe and within allowed directories
///
/// Prevents path traversal attacks by:
/// - Resolving the canonical path (follows symlinks)
/// - Checking it starts with one of the allowed directories
/// - Rejecting paths with ".." components
/// - Rejecting paths outside the allowed directories
pub fn validate_file_path(path: &Path) -> Result<PathBuf, GuideAIError> {
    // Check for ".." components before canonicalization
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(GuideAIError::Validation(format!(
                "Path contains '..' component: {}",
                path.display()
            )));
        }
    }

    // Get canonical path (resolves symlinks and relative paths)
    let canonical = path.canonicalize().map_err(|e| {
        GuideAIError::Validation(format!(
            "Failed to resolve path '{}': {}",
            path.display(),
            e
        ))
    })?;

    // Check if path is within allowed directories
    let allowed_dirs = get_allowed_directories()?;
    let is_allowed = allowed_dirs
        .iter()
        .any(|allowed_dir| canonical.starts_with(allowed_dir));

    if !is_allowed {
        return Err(GuideAIError::Validation(format!(
            "Path is outside allowed directories: {}",
            canonical.display()
        )));
    }

    Ok(canonical)
}

/// Validate file size is within the specified limit
pub fn validate_file_size(path: &Path, max_size: u64) -> Result<u64, GuideAIError> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        GuideAIError::Validation(format!(
            "Failed to get file metadata for '{}': {}",
            path.display(),
            e
        ))
    })?;

    let size = metadata.len();

    if size > max_size {
        return Err(GuideAIError::Validation(format!(
            "File size ({} bytes) exceeds maximum allowed size ({} bytes): {}",
            size,
            max_size,
            path.display()
        )));
    }

    Ok(size)
}

/// Validate both path and size for session files
pub fn validate_session_file(path: &Path) -> Result<(PathBuf, u64), GuideAIError> {
    let canonical_path = validate_file_path(path)?;
    let size = validate_file_size(&canonical_path, MAX_SESSION_FILE_SIZE)?;
    Ok((canonical_path, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_validate_path_with_parent_dir() {
        let path = PathBuf::from("../etc/passwd");
        let result = validate_file_path(&path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("'..' component"));
    }

    #[test]
    fn test_validate_file_size_exceeds_limit() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("large_file.txt");

        // Create a file larger than 1KB for testing
        let content = "x".repeat(2000);
        fs::write(&file_path, content).unwrap();

        let result = validate_file_size(&file_path, 1024);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_validate_file_size_within_limit() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("small_file.txt");

        fs::write(&file_path, "small content").unwrap();

        let result = validate_file_size(&file_path, 1024);
        assert!(result.is_ok());
        assert!(result.unwrap() > 0);
    }

    #[test]
    fn test_allowed_directories_exist() {
        let dirs = get_allowed_directories();
        assert!(dirs.is_ok());
        let dirs = dirs.unwrap();
        assert!(!dirs.is_empty());

        // Should include .guideai, .claude, .codex, and opencode
        assert!(dirs.len() >= 4);
    }
}
