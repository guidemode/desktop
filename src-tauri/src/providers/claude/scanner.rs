use crate::providers::sort_projects_by_modified;
use crate::config::ProjectInfo;
use chrono::{DateTime, Utc};
use shellexpand::tilde;
use std::fs;
use std::path::PathBuf;

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
        base_candidates.push(home_dir.join(".claude"));
    }

    // Find first existing base path
    let base_path = base_candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| format!("Claude home directory not found. Tried: {}", home_directory))?;

    let projects_path = base_path.join("projects");
    if !projects_path.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&projects_path)
        .map_err(|e| format!("Failed to read projects directory: {}", e))?;

    let mut projects = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
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
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
                last_modified: modified.to_rfc3339(),
            },
        ));
    }

    Ok(sort_projects_by_modified(projects))
}
