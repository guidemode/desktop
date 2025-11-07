use crate::config::ProjectInfo;
use chrono::{DateTime, Utc};

pub mod canonical; // Canonical format types and converter trait
pub mod claude; // Claude Code converter (public for canonical format migration)
pub mod codex; // Codex converter (public for canonical format migration)
pub mod common;
pub mod copilot; // Copilot converter (public for canonical format migration)
pub mod cursor; // Cursor converter
pub mod gemini; // Gemini converter (public for canonical format migration)
pub mod opencode; // OpenCode converter (public for canonical format migration)
mod session_scanner;

// Re-export watchers from provider modules
pub use claude::watcher::{ClaudeWatcher, ClaudeWatcherStatus};
pub use codex::watcher::{CodexWatcher, CodexWatcherStatus};
pub use copilot::watcher::{CopilotWatcher, CopilotWatcherStatus};
pub use cursor::watcher::{CursorWatcher, CursorWatcherStatus};
pub use gemini::watcher::{GeminiWatcher, GeminiWatcherStatus};
pub use opencode::watcher::{OpenCodeWatcher, OpenCodeWatcherStatus};
pub use session_scanner::{scan_all_sessions_filtered, SessionInfo};

pub fn scan_projects(provider_id: &str, home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    match provider_id {
        "claude-code" => claude::scan_projects(home_directory),
        "github-copilot" => copilot::utils::scan_projects(home_directory),
        "opencode" => opencode::scan_projects(home_directory),
        "codex" => codex::scan_projects(home_directory),
        "gemini-code" => gemini::utils::scan_projects(home_directory),
        "cursor" => cursor::scan_projects(home_directory),
        other => Err(format!("Unsupported provider: {}", other)),
    }
}

pub(super) fn sort_projects_by_modified(
    mut projects: Vec<(DateTime<Utc>, ProjectInfo)>,
) -> Vec<ProjectInfo> {
    projects.sort_by(|a, b| b.0.cmp(&a.0));
    projects.into_iter().map(|(_, info)| info).collect()
}
