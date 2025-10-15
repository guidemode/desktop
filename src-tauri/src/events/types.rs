use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Sequence number for ordering events
pub type EventSequence = u64;

/// All session-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub sequence: EventSequence,
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub payload: SessionEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventPayload {
    /// Session file changed (new or updated)
    /// Database handler will use db_helpers which does smart insert-or-update
    SessionChanged {
        session_id: String,
        project_name: String,
        file_path: PathBuf,
        file_size: u64,
    },

    /// Session completed (has end time)
    Completed {
        session_id: String,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        duration_ms: i64,
    },

    /// Session processing failed
    Failed {
        session_id: String,
        reason: String,
    },
}

impl SessionEvent {
    pub fn session_id(&self) -> &str {
        match &self.payload {
            SessionEventPayload::SessionChanged { session_id, .. } => session_id,
            SessionEventPayload::Completed { session_id, .. } => session_id,
            SessionEventPayload::Failed { session_id, .. } => session_id,
        }
    }

    pub fn payload_type(&self) -> &str {
        match &self.payload {
            SessionEventPayload::SessionChanged { .. } => "session_changed",
            SessionEventPayload::Completed { .. } => "completed",
            SessionEventPayload::Failed { .. } => "failed",
        }
    }
}
