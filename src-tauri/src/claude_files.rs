use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Type of Claude configuration file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ClaudeFileType {
    Command,
    Skill,
    Config,
    Other,
}

/// Metadata parsed from frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Claude file information returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeFile {
    pub file_name: String,     // e.g., "generate-schemas.md"
    pub file_path: String,     // Absolute path
    pub relative_path: String, // Path relative to .claude folder
    pub content: String,       // File contents
    pub size: u64,             // File size in bytes
    pub file_type: ClaudeFileType,
    pub metadata: Option<ClaudeMetadata>,
}

/// Scan .claude directory for all configuration files
pub fn scan_claude_files(cwd: &str) -> Result<Vec<ClaudeFile>, String> {
    let cwd_path = Path::new(cwd);
    let claude_dir = cwd_path.join(".claude");

    // Check if .claude directory exists
    if !claude_dir.exists() {
        return Ok(Vec::new()); // Return empty vec if no .claude folder
    }

    if !claude_dir.is_dir() {
        return Err(format!(".claude exists but is not a directory: {:?}", claude_dir));
    }

    let mut claude_files = Vec::new();

    // Scan commands directory
    let commands_dir = claude_dir.join("commands");
    if commands_dir.exists() && commands_dir.is_dir() {
        scan_commands(&commands_dir, &claude_dir, &mut claude_files)?;
    }

    // Scan skills directory
    let skills_dir = claude_dir.join("skills");
    if skills_dir.exists() && skills_dir.is_dir() {
        scan_skills(&skills_dir, &claude_dir, &mut claude_files)?;
    }

    // Scan for config files (JSON files in root of .claude)
    scan_config_files(&claude_dir, &mut claude_files)?;

    // Scan for any other markdown files in .claude root
    scan_other_files(&claude_dir, &mut claude_files)?;

    // Sort by file type, then by relative path
    claude_files.sort_by(|a, b| {
        match (&a.file_type, &b.file_type) {
            (ClaudeFileType::Command, ClaudeFileType::Command) => a.relative_path.cmp(&b.relative_path),
            (ClaudeFileType::Command, _) => std::cmp::Ordering::Less,
            (_, ClaudeFileType::Command) => std::cmp::Ordering::Greater,
            (ClaudeFileType::Skill, ClaudeFileType::Skill) => a.relative_path.cmp(&b.relative_path),
            (ClaudeFileType::Skill, _) => std::cmp::Ordering::Less,
            (_, ClaudeFileType::Skill) => std::cmp::Ordering::Greater,
            (ClaudeFileType::Config, ClaudeFileType::Config) => a.relative_path.cmp(&b.relative_path),
            (ClaudeFileType::Config, _) => std::cmp::Ordering::Less,
            (_, ClaudeFileType::Config) => std::cmp::Ordering::Greater,
            _ => a.relative_path.cmp(&b.relative_path),
        }
    });

    Ok(claude_files)
}

/// Scan commands directory for command files
fn scan_commands(
    commands_dir: &Path,
    claude_root: &Path,
    files: &mut Vec<ClaudeFile>,
) -> Result<(), String> {
    let entries = fs::read_dir(commands_dir)
        .map_err(|e| format!("Failed to read commands directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        // Only process .md files
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        match read_claude_file(&path, claude_root, ClaudeFileType::Command) {
            Ok(file) => files.push(file),
            Err(e) => {
                eprintln!("Failed to read command file {:?}: {}", path, e);
            }
        }
    }

    Ok(())
}

/// Scan skills directory for skill files
fn scan_skills(
    skills_dir: &Path,
    claude_root: &Path,
    files: &mut Vec<ClaudeFile>,
) -> Result<(), String> {
    let entries = fs::read_dir(skills_dir)
        .map_err(|e| format!("Failed to read skills directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        // Each skill is a directory with SKILL.md inside
        if !path.is_dir() {
            continue;
        }

        let skill_file = path.join("SKILL.md");
        if skill_file.exists() && skill_file.is_file() {
            match read_claude_file(&skill_file, claude_root, ClaudeFileType::Skill) {
                Ok(file) => files.push(file),
                Err(e) => {
                    eprintln!("Failed to read skill file {:?}: {}", skill_file, e);
                }
            }
        }
    }

    Ok(())
}

/// Scan .claude root for config files (JSON files)
fn scan_config_files(claude_root: &Path, files: &mut Vec<ClaudeFile>) -> Result<(), String> {
    let entries = fs::read_dir(claude_root)
        .map_err(|e| format!("Failed to read .claude directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        // Only process JSON files in root
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        match read_claude_file(&path, claude_root, ClaudeFileType::Config) {
            Ok(file) => files.push(file),
            Err(e) => {
                eprintln!("Failed to read config file {:?}: {}", path, e);
            }
        }
    }

    Ok(())
}

/// Scan .claude root for other markdown files
fn scan_other_files(claude_root: &Path, files: &mut Vec<ClaudeFile>) -> Result<(), String> {
    let entries = fs::read_dir(claude_root)
        .map_err(|e| format!("Failed to read .claude directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        // Only process markdown files in root (not in commands/skills subdirs)
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        match read_claude_file(&path, claude_root, ClaudeFileType::Other) {
            Ok(file) => files.push(file),
            Err(e) => {
                eprintln!("Failed to read file {:?}: {}", path, e);
            }
        }
    }

    Ok(())
}

/// Read a Claude file and extract metadata
fn read_claude_file(
    path: &Path,
    claude_root: &Path,
    file_type: ClaudeFileType,
) -> Result<ClaudeFile, String> {
    // Read file contents
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read file {:?}: {}", path, e))?;

    // Get file metadata
    let metadata =
        fs::metadata(path).map_err(|e| format!("Failed to get metadata for {:?}: {}", path, e))?;

    // Calculate relative path from .claude root
    let relative_path = path
        .strip_prefix(claude_root)
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

    // Parse frontmatter for commands and skills
    let parsed_metadata = if file_type == ClaudeFileType::Command || file_type == ClaudeFileType::Skill {
        parse_frontmatter(&content)
    } else {
        None
    };

    Ok(ClaudeFile {
        file_name,
        file_path,
        relative_path,
        content,
        size: metadata.len(),
        file_type,
        metadata: parsed_metadata,
    })
}

/// Parse frontmatter from markdown content
/// Supports simple YAML frontmatter between --- markers
fn parse_frontmatter(content: &str) -> Option<ClaudeMetadata> {
    let lines: Vec<&str> = content.lines().collect();

    // Check if content starts with ---
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }

    // Find closing ---
    let mut end_index = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_index = Some(i);
            break;
        }
    }

    let end_index = end_index?;

    // Parse YAML frontmatter (simple key: value parser)
    let mut frontmatter: HashMap<String, String> = HashMap::new();

    for line in &lines[1..end_index] {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
            frontmatter.insert(key, value);
        }
    }

    Some(ClaudeMetadata {
        name: frontmatter.get("name").cloned(),
        description: frontmatter.get("description").cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scan_empty_claude_directory() {
        let temp_dir = TempDir::new().unwrap();
        let result = scan_claude_files(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_scan_with_command() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        let claude_dir = temp_path.join(".claude");
        let commands_dir = claude_dir.join("commands");
        fs::create_dir_all(&commands_dir).unwrap();

        let command_content = r#"---
description: Test command
---
This is a test command."#;

        fs::write(commands_dir.join("test.md"), command_content).unwrap();

        let result = scan_claude_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_type, ClaudeFileType::Command);
        assert!(files[0].metadata.is_some());
        assert_eq!(
            files[0].metadata.as_ref().unwrap().description.as_deref(),
            Some("Test command")
        );
    }

    #[test]
    fn test_scan_with_skill() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        let claude_dir = temp_path.join(".claude");
        let skills_dir = claude_dir.join("skills").join("test-skill");
        fs::create_dir_all(&skills_dir).unwrap();

        let skill_content = r#"---
name: test-skill
description: Test skill
---
This is a test skill."#;

        fs::write(skills_dir.join("SKILL.md"), skill_content).unwrap();

        let result = scan_claude_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_type, ClaudeFileType::Skill);
        assert!(files[0].metadata.is_some());
        assert_eq!(
            files[0].metadata.as_ref().unwrap().name.as_deref(),
            Some("test-skill")
        );
        assert_eq!(
            files[0].metadata.as_ref().unwrap().description.as_deref(),
            Some("Test skill")
        );
    }

    #[test]
    fn test_scan_with_config() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        let claude_dir = temp_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let config_content = r#"{"key": "value"}"#;
        fs::write(claude_dir.join("settings.local.json"), config_content).unwrap();

        let result = scan_claude_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_type, ClaudeFileType::Config);
        assert!(files[0].metadata.is_none());
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: test
description: Test description
---
Body content"#;

        let metadata = parse_frontmatter(content);
        assert!(metadata.is_some());
        let metadata = metadata.unwrap();
        assert_eq!(metadata.name.as_deref(), Some("test"));
        assert_eq!(metadata.description.as_deref(), Some("Test description"));
    }

    #[test]
    fn test_parse_frontmatter_no_markers() {
        let content = "Just plain content";
        let metadata = parse_frontmatter(content);
        assert!(metadata.is_none());
    }

    #[test]
    fn test_file_type_sorting() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        let claude_dir = temp_path.join(".claude");
        let commands_dir = claude_dir.join("commands");
        let skills_dir = claude_dir.join("skills").join("test-skill");
        fs::create_dir_all(&commands_dir).unwrap();
        fs::create_dir_all(&skills_dir).unwrap();

        fs::write(commands_dir.join("command.md"), "---\ndescription: Command\n---\nCommand").unwrap();
        fs::write(skills_dir.join("SKILL.md"), "---\nname: skill\ndescription: Skill\n---\nSkill").unwrap();
        fs::write(claude_dir.join("settings.json"), "{}").unwrap();

        let result = scan_claude_files(temp_path.to_str().unwrap());
        assert!(result.is_ok());
        let files = result.unwrap();

        // Should be sorted: Commands, Skills, Config
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].file_type, ClaudeFileType::Command);
        assert_eq!(files[1].file_type, ClaudeFileType::Skill);
        assert_eq!(files[2].file_type, ClaudeFileType::Config);
    }
}
