pub mod converter;

pub use converter::convert_opencode_jsonl_to_canonical;

use super::opencode_parser::OpenCodeParser;
use super::sort_projects_by_modified;
use crate::config::ProjectInfo;
use chrono::{DateTime, Utc};
use shellexpand::tilde;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn scan_projects(home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    let expanded = tilde(home_directory);
    let primary_base = PathBuf::from(expanded.into_owned());

    let mut base_candidates = Vec::new();

    let mut push_candidate = |candidate: PathBuf| {
        if !base_candidates
            .iter()
            .any(|existing| existing == &candidate)
        {
            base_candidates.push(candidate);
        }
    };

    push_candidate(primary_base.clone());

    if let Some(parent) = primary_base.parent() {
        if primary_base
            .file_name()
            .map(|name| name == OsStr::new(".opencode"))
            .unwrap_or(false)
        {
            push_candidate(parent.join(".local/share/opencode"));
        }
    }

    if let Some(home_dir) = dirs::home_dir() {
        push_candidate(home_dir.join(".local/share/opencode"));
    }

    if let Some(data_dir) = dirs::data_dir() {
        push_candidate(data_dir.join("opencode"));
    }

    let storage_dir = base_candidates.into_iter().find_map(|base| {
        let storage = base.join("storage");
        storage.is_dir().then_some(storage)
    });

    let Some(storage_path) = storage_dir else {
        return Ok(Vec::new());
    };

    // Use the new OpenCode parser for better project discovery
    let parser = OpenCodeParser::new(storage_path);

    let opencode_projects = parser
        .get_all_projects()
        .map_err(|e| format!("Failed to get OpenCode projects: {}", e))?;

    let mut projects = Vec::new();

    for project in opencode_projects {
        let worktree_path = Path::new(&project.worktree);
        if !worktree_path.exists() {
            continue;
        }

        let Some(name) = worktree_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // Get the most recent session activity for this project
        let session_ids = parser
            .get_sessions_for_project(&project.id)
            .unwrap_or_default();

        let mut most_recent_activity = project
            .time
            .updated
            .or(project.time.initialized)
            .or(project.time.created)
            .and_then(DateTime::<Utc>::from_timestamp_millis);

        // Find the most recent session activity
        for session_id in session_ids {
            if let Ok(parsed_session) = parser.parse_session(&session_id) {
                if let Some(session_end) = parsed_session.session_end_time {
                    match most_recent_activity {
                        Some(current_latest) => {
                            if session_end > current_latest {
                                most_recent_activity = Some(session_end);
                            }
                        }
                        None => {
                            most_recent_activity = Some(session_end);
                        }
                    }
                }
            }
        }

        // Fallback to file system metadata
        let metadata_time = fs::metadata(worktree_path)
            .and_then(|metadata| metadata.modified())
            .map(DateTime::<Utc>::from)
            .ok();

        let modified = most_recent_activity
            .or(metadata_time)
            .unwrap_or_else(|| DateTime::<Utc>::from(SystemTime::UNIX_EPOCH));

        projects.push((
            modified,
            ProjectInfo {
                name: name.to_string(),
                path: worktree_path.to_string_lossy().to_string(),
                last_modified: modified.to_rfc3339(),
            },
        ));
    }

    Ok(sort_projects_by_modified(projects))
}
