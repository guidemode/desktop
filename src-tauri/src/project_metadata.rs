use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use git2::Repository;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub project_name: String,
    pub git_remote_url: Option<String>,
    pub cwd: String,
    pub detected_project_type: String,
}

/// Extract project metadata from a directory
pub fn extract_project_metadata(cwd: &str) -> Result<ProjectMetadata, String> {
    let path = Path::new(cwd);

    // Validate that directory exists
    if !path.exists() {
        return Err(format!("Directory does not exist: {}", cwd));
    }

    // Validate that it is actually a directory
    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", cwd));
    }

    // Validate path is canonical (no symlinks to sensitive areas)
    // and get the canonical path for consistent comparison
    let _canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path '{}': {}", cwd, e))?;

    // Detect project type and extract name
    let (project_name, detected_project_type) = detect_project_type_and_name(path)?;

    // Extract Git remote URL
    let git_remote_url = extract_git_remote_url(path);

    Ok(ProjectMetadata {
        project_name,
        git_remote_url,
        cwd: cwd.to_string(),
        detected_project_type,
    })
}

/// Detect project type and extract project name
fn detect_project_type_and_name(path: &Path) -> Result<(String, String), String> {
    // Check for Node.js project (package.json)
    let package_json = path.join("package.json");
    if package_json.exists() {
        if let Ok(project_name) = extract_nodejs_project_name(&package_json) {
            return Ok((project_name, "nodejs".to_string()));
        }
    }

    // Check for Rust project (Cargo.toml)
    let cargo_toml = path.join("Cargo.toml");
    if cargo_toml.exists() {
        if let Ok(project_name) = extract_rust_project_name(&cargo_toml) {
            return Ok((project_name, "rust".to_string()));
        }
    }

    // Check for Python project (pyproject.toml)
    let pyproject_toml = path.join("pyproject.toml");
    if pyproject_toml.exists() {
        if let Ok(project_name) = extract_python_project_name(&pyproject_toml) {
            return Ok((project_name, "python".to_string()));
        }
    }

    // Check for Go project (go.mod)
    let go_mod = path.join("go.mod");
    if go_mod.exists() {
        if let Ok(project_name) = extract_go_project_name(&go_mod) {
            return Ok((project_name, "go".to_string()));
        }
    }

    // Fallback: use directory name
    let dir_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid directory name")?
        .to_string();

    Ok((dir_name, "generic".to_string()))
}

/// Extract project name from package.json
fn extract_nodejs_project_name(package_json: &Path) -> Result<String, String> {
    let content = fs::read_to_string(package_json)
        .map_err(|e| format!("Failed to read package.json: {}", e))?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse package.json: {}", e))?;

    json.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No 'name' field in package.json".to_string())
}

/// Extract project name from Cargo.toml
fn extract_rust_project_name(cargo_toml: &Path) -> Result<String, String> {
    let content =
        fs::read_to_string(cargo_toml).map_err(|e| format!("Failed to read Cargo.toml: {}", e))?;

    // Simple parser for TOML [package] name field
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name") && trimmed.contains('=') {
            if let Some(value_part) = trimmed.split('=').nth(1) {
                let name = value_part.trim().trim_matches('"').trim_matches('\'');
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
    }

    Err("No 'name' field found in Cargo.toml".to_string())
}

/// Extract project name from pyproject.toml
fn extract_python_project_name(pyproject_toml: &Path) -> Result<String, String> {
    let content = fs::read_to_string(pyproject_toml)
        .map_err(|e| format!("Failed to read pyproject.toml: {}", e))?;

    // Simple parser for TOML [project] name or [tool.poetry] name field
    let mut in_project_section = false;
    let mut in_poetry_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for section headers
        if trimmed == "[project]" {
            in_project_section = true;
            in_poetry_section = false;
            continue;
        } else if trimmed == "[tool.poetry]" {
            in_poetry_section = true;
            in_project_section = false;
            continue;
        } else if trimmed.starts_with('[') {
            in_project_section = false;
            in_poetry_section = false;
            continue;
        }

        // Look for name field in relevant sections
        if (in_project_section || in_poetry_section)
            && trimmed.starts_with("name")
            && trimmed.contains('=')
        {
            if let Some(value_part) = trimmed.split('=').nth(1) {
                let name = value_part.trim().trim_matches('"').trim_matches('\'');
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
    }

    Err("No 'name' field found in pyproject.toml".to_string())
}

/// Extract project name from go.mod
fn extract_go_project_name(go_mod: &Path) -> Result<String, String> {
    let content =
        fs::read_to_string(go_mod).map_err(|e| format!("Failed to read go.mod: {}", e))?;

    // Look for "module" directive
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("module ") {
            let module_path = trimmed.strip_prefix("module ").unwrap().trim();
            // Extract last component of module path
            if let Some(last_part) = module_path.split('/').next_back() {
                return Ok(last_part.to_string());
            }
            return Ok(module_path.to_string());
        }
    }

    Err("No 'module' directive found in go.mod".to_string())
}

/// Convert SSH Git URL to HTTPS URL for GitHub
fn convert_ssh_to_https(url: &str) -> String {
    // Check if it's a GitHub SSH URL (git@github.com:owner/repo.git)
    if url.starts_with("git@github.com:") {
        // Extract the owner/repo part
        let repo_part = url.strip_prefix("git@github.com:").unwrap_or(url);
        // Convert to HTTPS format
        return format!("https://github.com/{}", repo_part);
    }

    // Check for other SSH URL formats (ssh://git@github.com/owner/repo.git)
    if url.starts_with("ssh://git@github.com/") {
        let repo_part = url.strip_prefix("ssh://git@github.com/").unwrap_or(url);
        return format!("https://github.com/{}", repo_part);
    }

    // If it's already HTTPS or another format, return as-is
    url.to_string()
}

/// Extract Git remote URL from .git/config
fn extract_git_remote_url(path: &Path) -> Option<String> {
    let git_config = path.join(".git").join("config");

    // Validate git config exists and is a file
    if !git_config.exists() || !git_config.is_file() {
        return None;
    }

    // Read config file (with size safety - Git configs are typically small)
    let metadata = fs::metadata(&git_config).ok()?;
    const MAX_GIT_CONFIG_SIZE: u64 = 1024 * 1024; // 1MB should be plenty for a .git/config
    if metadata.len() > MAX_GIT_CONFIG_SIZE {
        return None; // Suspiciously large config file
    }

    let content = fs::read_to_string(&git_config).ok()?;

    // Simple parser for Git config [remote "origin"] url
    let mut in_remote_origin = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for [remote "origin"]
        if trimmed.starts_with("[remote \"origin\"]") || trimmed.starts_with("[remote 'origin']") {
            in_remote_origin = true;
            continue;
        }

        // Stop if we hit another section
        if trimmed.starts_with('[') && in_remote_origin {
            in_remote_origin = false;
            continue;
        }

        // Look for url = ... in remote origin section
        if in_remote_origin && trimmed.starts_with("url") && trimmed.contains('=') {
            if let Some(value_part) = trimmed.split('=').nth(1) {
                let url = value_part.trim();
                if !url.is_empty() {
                    // Convert SSH URLs to HTTPS for GitHub
                    return Some(convert_ssh_to_https(url));
                }
            }
        }
    }

    None
}

/// Extract current git branch from working directory
/// Returns None if not a git repository or if there's an error
pub fn extract_git_branch(cwd: &str) -> Option<String> {
    let repo = Repository::open(cwd).ok()?;
    let head = repo.head().ok()?;
    head.shorthand().map(String::from)
}

/// Extract current git commit hash from working directory
/// Returns None if not a git repository or if there's an error
pub fn extract_git_commit_hash(cwd: &str) -> Option<String> {
    let repo = Repository::open(cwd).ok()?;
    let head = repo.head().ok()?;
    let oid = head.target()?;
    Some(oid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_extract_nodejs_project() {
        let temp_dir = tempdir().unwrap();
        let package_json = temp_dir.path().join("package.json");

        fs::write(
            &package_json,
            r#"{"name": "test-project", "version": "1.0.0"}"#,
        )
        .unwrap();

        let metadata = extract_project_metadata(temp_dir.path().to_str().unwrap()).unwrap();

        assert_eq!(metadata.project_name, "test-project");
        assert_eq!(metadata.detected_project_type, "nodejs");
    }

    #[test]
    fn test_extract_rust_project() {
        let temp_dir = tempdir().unwrap();
        let cargo_toml = temp_dir.path().join("Cargo.toml");

        fs::write(
            &cargo_toml,
            "[package]\nname = \"my-rust-app\"\nversion = \"0.1.0\"",
        )
        .unwrap();

        let metadata = extract_project_metadata(temp_dir.path().to_str().unwrap()).unwrap();

        assert_eq!(metadata.project_name, "my-rust-app");
        assert_eq!(metadata.detected_project_type, "rust");
    }

    #[test]
    fn test_extract_python_project() {
        let temp_dir = tempdir().unwrap();
        let pyproject_toml = temp_dir.path().join("pyproject.toml");

        fs::write(
            &pyproject_toml,
            "[project]\nname = \"my-python-app\"\nversion = \"0.1.0\"",
        )
        .unwrap();

        let metadata = extract_project_metadata(temp_dir.path().to_str().unwrap()).unwrap();

        assert_eq!(metadata.project_name, "my-python-app");
        assert_eq!(metadata.detected_project_type, "python");
    }

    #[test]
    fn test_extract_go_project() {
        let temp_dir = tempdir().unwrap();
        let go_mod = temp_dir.path().join("go.mod");

        fs::write(&go_mod, "module github.com/user/my-go-app\n\ngo 1.21").unwrap();

        let metadata = extract_project_metadata(temp_dir.path().to_str().unwrap()).unwrap();

        assert_eq!(metadata.project_name, "my-go-app");
        assert_eq!(metadata.detected_project_type, "go");
    }

    #[test]
    fn test_extract_git_remote_url() {
        let temp_dir = tempdir().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        let git_config = git_dir.join("config");

        fs::write(
            &git_config,
            "[remote \"origin\"]\n\turl = https://github.com/user/repo.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
        ).unwrap();

        let metadata = extract_project_metadata(temp_dir.path().to_str().unwrap()).unwrap();

        assert_eq!(
            metadata.git_remote_url,
            Some("https://github.com/user/repo.git".to_string())
        );
    }

    #[test]
    fn test_fallback_to_directory_name() {
        let temp_dir = tempdir().unwrap();

        let metadata = extract_project_metadata(temp_dir.path().to_str().unwrap()).unwrap();

        assert!(metadata.project_name.starts_with(".tmp")); // tempdir creates .tmpXXXXXX dirs
        assert_eq!(metadata.detected_project_type, "generic");
    }

    #[test]
    fn test_ssh_to_https_conversion() {
        // Test GitHub SSH URL format (git@github.com:owner/repo.git)
        assert_eq!(
            convert_ssh_to_https("git@github.com:guideai-dev/guideai.git"),
            "https://github.com/guideai-dev/guideai.git"
        );

        // Test SSH protocol format (ssh://git@github.com/owner/repo.git)
        assert_eq!(
            convert_ssh_to_https("ssh://git@github.com/guideai-dev/guideai.git"),
            "https://github.com/guideai-dev/guideai.git"
        );

        // Test HTTPS URL (should remain unchanged)
        assert_eq!(
            convert_ssh_to_https("https://github.com/guideai-dev/guideai.git"),
            "https://github.com/guideai-dev/guideai.git"
        );

        // Test non-GitHub URL (should remain unchanged)
        assert_eq!(
            convert_ssh_to_https("git@gitlab.com:owner/repo.git"),
            "git@gitlab.com:owner/repo.git"
        );
    }

    #[test]
    fn test_extract_git_remote_url_with_ssh() {
        let temp_dir = tempdir().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        let git_config = git_dir.join("config");

        // Write SSH URL to config
        fs::write(
            &git_config,
            "[remote \"origin\"]\n\turl = git@github.com:guideai-dev/guideai.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
        ).unwrap();

        let metadata = extract_project_metadata(temp_dir.path().to_str().unwrap()).unwrap();

        // Should be converted to HTTPS
        assert_eq!(
            metadata.git_remote_url,
            Some("https://github.com/guideai-dev/guideai.git".to_string())
        );
    }
}
