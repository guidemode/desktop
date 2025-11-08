// Common utilities shared across all provider watchers
//
// This module contains shared code that was previously duplicated
// across Claude, Claude Code, Copilot, Cursor, and Gemini Code watchers.

pub mod agent_merger;
pub mod canonical_path;
pub mod constants;
pub mod db_helpers;
pub mod file_utils;
pub mod session_info;
pub mod session_state;
pub mod timing;
pub mod watcher_status;

// Re-export commonly used types
pub use agent_merger::*;
pub use canonical_path::*;
pub use constants::*;
pub use file_utils::*;
pub use session_info::SessionInfo;
pub use session_state::SessionStateManager;
pub use watcher_status::WatcherStatus;
