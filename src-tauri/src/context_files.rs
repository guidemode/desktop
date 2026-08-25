use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Context file information returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub file_name: String,     // e.g., "CLAUDE.md"
    pub file_path: String,     // Absolute path
    pub relative_path: String, // Path relative to cwd
    pub content: String,       // File contents
    pub size: u64,             // File size in bytes
}

/// Context file names to search for (case-insensitive)
const CONTEXT_FILE_NAMES: &[&str] = &["CLAUDE.md", "AGENTS.md", "GEMINI.md"];

/// Scan a directory for context files, honoring .gitignore patterns
pub fn scan_context_files(cwd: &str) -> Result<Vec<ContextFile>, String> {
    let cwd_path = Path::new(cwd);

    // Check if directory exists
    if !cwd_path.exists() {
        return Err(format!("Directory does not exist: {}", cwd));
    }

    if !cwd_path.is_dir() {
        return Err(format!("Path is not a directory: {}", cwd));
    }

    let mut context_files = Vec::new();

    // Use ignore crate's WalkBuilder to respect .gitignore
    let walker = WalkBuilder::new(cwd_path)
        .standard_filters(true) // Enables .gitignore, .ignore, .git/info/exclude
        .hidden(false) // Include hidden files (some projects put context in .github/)
        .follow_links(false) // Don't follow symlinks
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();

                // Only process files (not directories)
                if !path.is_file() {
                    continue;
                }

                // Check if filename matches any context file (case-insensitive)
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    let file_name_lower = file_name.to_lowercase();
                    let is_context_file = CONTEXT_FILE_NAMES
                        .iter()
                        .any(|name| name.to_lowercase() == file_name_lower);

                    if is_context_file {
                        match read_context_file(path, cwd_path) {
                            Ok(context_file) => context_files.push(context_file),
                            Err(e) => {
                                // Log error but continue scanning
                                eprintln!("Failed to read context file {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Log error but continue scanning
                eprintln!("Error walking directory: {}", e);
            }
        }
    }

    // Sort by relative path for consistent ordering
    context_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(context_files)
}

/// Read a context file and return its information
fn read_context_file(path: &Path, cwd: &Path) -> Result<ContextFile, String> {
    // Read file contents
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read file {:?}: {}", path, e))?;

    // Get file metadata
    let metadata =
        fs::metadata(path).map_err(|e| format!("Failed to get metadata for {:?}: {}", path, e))?;

    // Calculate relative path
    let relative_path = path
        .strip_prefix(cwd)
        .map_err(|e| format!("Failed to calculate relative path: {}", e))?
        .to_string_lossy()
        .to_string();

    // Get file name
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Failed to get file name for {:?}", path))?
        .to_string();

    // Get absolute path
    let file_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize path {:?}: {}", path, e))?
        .to_string_lossy()
        .to_string();

    Ok(ContextFile {
        file_name,
        file_path,
        relative_path,
        content,
        size: metadata.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let result = scan_context_files(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_scan_with_context_files() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create context files
        fs::write(temp_path.join("CLAUDE.md"), "Claude instructions").unwrap();
        fs::write(temp_path.join("AGENTS.md"), "Agent config").unwrap();

        let result = scan_context_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create files with different case
        fs::write(temp_path.join("claude.md"), "Lower case").unwrap();
        fs::write(temp_path.join("CLAUDE.MD"), "Upper case").unwrap();

        let result = scan_context_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert!(!files.is_empty()); // At least one should match
    }

    #[test]
    fn test_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create nested structure
        let nested_dir = temp_path.join("src").join("docs");
        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(nested_dir.join("CLAUDE.md"), "Nested file").unwrap();

        let result = scan_context_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.contains("src"));
    }

    #[test]
    fn test_gitignore_respected() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create .git directory (required for ignore crate to respect .gitignore)
        fs::create_dir(temp_path.join(".git")).unwrap();

        // Create .gitignore
        fs::write(temp_path.join(".gitignore"), "ignored/\n").unwrap();

        // Create files in ignored directory
        let ignored_dir = temp_path.join("ignored");
        fs::create_dir(&ignored_dir).unwrap();
        fs::write(ignored_dir.join("CLAUDE.md"), "Should be ignored").unwrap();

        // Create file in root
        fs::write(temp_path.join("CLAUDE.md"), "Not ignored").unwrap();

        let result = scan_context_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();

        // Should only find the root file, not the ignored one
        assert_eq!(files.len(), 1);
        assert!(!files[0].relative_path.contains("ignored"));
    }
}
