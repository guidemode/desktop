use serde::{Deserialize, Serialize};

/// Generic watcher status that all providers share
/// This replaces the individual ClaudeWatcherStatus, CopilotWatcherStatus, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherStatus {
    pub is_running: bool,
    pub pending_uploads: usize,
    pub processing_uploads: usize,
    pub failed_uploads: usize,
}
