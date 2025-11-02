use crate::config::ProjectInfo;
use crate::providers::copilot_parser::load_copilot_config;
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
        base_candidates.push(home_dir.join(".copilot"));
    }

    // Find first existing base path
    let base_path = base_candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            format!(
                "GitHub Copilot home directory not found. Tried: {}",
                home_directory
            )
        })?;

    // Load the copilot config to get trusted folders
    let copilot_config = load_copilot_config().unwrap_or_else(|e| {
        eprintln!("Warning: Failed to load copilot config: {}", e);
        Default::default()
    });

    let mut projects = Vec::new();

    // If we have trusted folders, use them as projects
    if !copilot_config.trusted_folders.is_empty() {
        for folder in &copilot_config.trusted_folders {
            let expanded_folder = tilde(folder);
            let folder_path = PathBuf::from(expanded_folder.into_owned());

            // Get the folder name (last component of the path)
            if let Some(name) = folder_path.file_name().and_then(|n| n.to_str()) {
                // Get the last modified time if the folder exists
                let last_modified = if folder_path.exists() {
                    fs::metadata(&folder_path)
                        .and_then(|m| m.modified())
                        .ok()
                        .map(|modified| {
                            let dt: DateTime<Utc> = modified.into();
                            dt
                        })
                        .unwrap_or_else(Utc::now)
                } else {
                    Utc::now()
                };

                projects.push(ProjectInfo {
                    name: name.to_string(),
                    path: folder_path.to_string_lossy().to_string(),
                    last_modified: last_modified.to_rfc3339(),
                });
            }
        }
    }

    // If no trusted folders found or no projects added, fall back to generic "copilot-sessions"
    if projects.is_empty() {
        // GitHub Copilot doesn't have a traditional projects structure
        // Sessions are stored in session-state directory
        // We'll create a synthetic "copilot-sessions" project
        let session_dir = base_path.join("session-state");
        if !session_dir.exists() {
            return Ok(Vec::new());
        }

        // Get the most recent modification time from any session file
        let entries = fs::read_dir(&session_dir)
            .map_err(|e| format!("Failed to read session directory: {}", e))?;

        let mut most_recent: Option<DateTime<Utc>> = None;
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let modified: DateTime<Utc> = modified.into();
                    most_recent = Some(match most_recent {
                        Some(existing) => existing.max(modified),
                        None => modified,
                    });
                }
            }
        }

        let last_modified = most_recent.unwrap_or_else(Utc::now);

        // Return a single "project" representing all Copilot sessions
        let project_info = ProjectInfo {
            name: "copilot-sessions".to_string(),
            path: session_dir.to_string_lossy().to_string(),
            last_modified: last_modified.to_rfc3339(),
        };

        projects.push(project_info);
    }

    Ok(projects)
}
