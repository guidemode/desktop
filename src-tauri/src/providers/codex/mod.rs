use super::sort_projects_by_modified;
use crate::config::ProjectInfo;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use shellexpand::tilde;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use toml::Value;

pub mod converter;

pub use converter::{CodexMessage, MessageAggregator};

#[derive(Debug, Deserialize, Default)]
struct CodexConfig {
    #[serde(default)]
    projects: HashMap<String, Value>,
}

pub fn scan_projects(home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    let expanded = tilde(home_directory);
    let primary_base = PathBuf::from(expanded.into_owned());

    // Build candidate paths with fallbacks for cross-platform support
    let mut base_candidates = Vec::new();

    // Add the primary path
    base_candidates.push(primary_base.clone());

    // Add platform-specific fallbacks
    if let Some(home_dir) = dirs::home_dir() {
        // Standard home directory path
        base_candidates.push(home_dir.join(".codex"));
    }

    // Find first existing base path
    let base_path = base_candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| format!("Codex home directory not found. Tried: {}", home_directory))?;

    let config_path = base_path.join("config.toml");
    if !config_path.exists() {
        return Ok(Vec::new());
    }

    let config_contents = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read Codex config: {}", e))?;

    let config: CodexConfig = toml::from_str(&config_contents)
        .map_err(|e| format!("Failed to parse Codex config: {}", e))?;

    if config.projects.is_empty() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();

    for (worktree, _details) in config.projects {
        let worktree = worktree.trim();
        if worktree.is_empty() {
            continue;
        }

        let path = Path::new(worktree);
        if !path.exists() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        let metadata_time = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .map(DateTime::<Utc>::from)
            .ok();

        let modified =
            metadata_time.unwrap_or_else(|| DateTime::<Utc>::from(SystemTime::UNIX_EPOCH));

        projects.push((
            modified,
            ProjectInfo {
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
                last_modified: modified.to_rfc3339(),
            },
        ));
    }

    Ok(sort_projects_by_modified(projects))
}
