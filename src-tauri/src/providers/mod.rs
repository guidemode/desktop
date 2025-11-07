use crate::config::ProjectInfo;
use chrono::{DateTime, Utc};

pub mod canonical; // Canonical format types and converter trait
pub mod claude; // Claude Code converter (public for canonical format migration)
mod claude_watcher;
pub mod codex; // Codex converter (public for canonical format migration)
mod codex_watcher;
pub mod common;
pub mod copilot; // Copilot converter (public for canonical format migration)
mod copilot_parser;
mod copilot_utils;
mod copilot_watcher;
pub mod cursor; // Cursor converter
mod cursor_watcher;
pub mod db_helpers;
pub mod gemini; // Gemini converter (public for canonical format migration)
pub mod gemini_parser; // Made public for test access
pub mod gemini_registry; // Registry for hash->CWD mappings
pub mod gemini_utils; // Made public for test access to CWD extraction functions
mod gemini_watcher;
pub mod opencode; // OpenCode converter (public for canonical format migration)
pub mod opencode_parser; // Made public for converter access
mod opencode_watcher;
mod session_scanner;

pub use claude_watcher::{ClaudeWatcher, ClaudeWatcherStatus};
pub use codex_watcher::{CodexWatcher, CodexWatcherStatus};
pub use copilot_watcher::{CopilotWatcher, CopilotWatcherStatus};
pub use cursor_watcher::{CursorWatcher, CursorWatcherStatus};
pub use gemini_watcher::{GeminiWatcher, GeminiWatcherStatus};
pub use opencode_watcher::{OpenCodeWatcher, OpenCodeWatcherStatus};
pub use session_scanner::{scan_all_sessions_filtered, SessionInfo};

pub fn scan_projects(provider_id: &str, home_directory: &str) -> Result<Vec<ProjectInfo>, String> {
    match provider_id {
        "claude-code" => claude::scan_projects(home_directory),
        "github-copilot" => copilot_utils::scan_projects(home_directory),
        "opencode" => opencode::scan_projects(home_directory),
        "codex" => codex::scan_projects(home_directory),
        "gemini-code" => gemini_utils::scan_projects(home_directory),
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
