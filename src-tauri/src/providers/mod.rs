use crate::config::ProjectInfo;
use chrono::{DateTime, Utc};

mod claude;
mod claude_watcher;
mod codex;
mod codex_watcher;
mod copilot;
mod copilot_parser;
mod copilot_snapshot;
mod copilot_watcher;
pub mod db_helpers;
mod gemini;
mod gemini_parser;
mod gemini_watcher;
mod opencode;
mod opencode_parser;
mod opencode_watcher;
mod session_scanner;

pub use claude_watcher::{ClaudeWatcher, ClaudeWatcherStatus};
pub use codex_watcher::{CodexWatcher, CodexWatcherStatus};
pub use copilot_watcher::{CopilotWatcher, CopilotWatcherStatus};
pub use gemini_watcher::{GeminiWatcher, GeminiWatcherStatus};
pub use opencode_watcher::{OpenCodeWatcher, OpenCodeWatcherStatus};
pub use session_scanner::{scan_all_sessions, SessionInfo};

pub fn scan_projects(provider_id: &str, home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    match provider_id {
        "claude-code" => claude::scan_projects(home_directory),
        "github-copilot" => copilot::scan_projects(home_directory),
        "opencode" => opencode::scan_projects(home_directory),
        "codex" => codex::scan_projects(home_directory),
        "gemini-code" => gemini::scan_projects(home_directory),
        other => Err(format!("Unsupported provider: {}", other)),
    }
}

pub(super) fn sort_projects_by_modified(
    mut projects: Vec<(DateTime<Utc>, ProjectInfo)>,
) -> Vec<ProjectInfo> {
    projects.sort_by(|a, b| b.0.cmp(&a.0));
    projects.into_iter().map(|(_, info)| info).collect()
}
