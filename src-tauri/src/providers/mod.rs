use crate::config::ProjectInfo;
use chrono::{DateTime, Utc};

mod claude;
mod claude_watcher;
mod codex;
mod opencode;
mod opencode_parser;
mod opencode_watcher;
mod session_scanner;

pub use claude_watcher::{ClaudeWatcher, ClaudeWatcherStatus};
pub use opencode_watcher::{OpenCodeWatcher, OpenCodeWatcherStatus};
pub use session_scanner::{SessionInfo, scan_all_sessions};

pub fn scan_projects(provider_id: &str, home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    match provider_id {
        "claude-code" => claude::scan_projects(home_directory),
        "opencode" => opencode::scan_projects(home_directory),
        "codex" => codex::scan_projects(home_directory),
        other => Err(format!("Unsupported provider: {}", other)),
    }
}

pub(super) fn sort_projects_by_modified(
    mut projects: Vec<(DateTime<Utc>, ProjectInfo)>,
) -> Vec<ProjectInfo> {
    projects.sort_by(|a, b| b.0.cmp(&a.0));
    projects.into_iter().map(|(_, info)| info).collect()
}
