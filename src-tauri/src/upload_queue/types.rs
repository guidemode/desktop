//! Type definitions for the upload queue system.
//!
//! Defines core data structures: UploadItem, UploadStatus, QueueItems, and constants.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Database polling interval (10 seconds by default, configurable later)
pub const DB_POLL_INTERVAL_SECS: u64 = 10;

// Maximum number of concurrent uploads (can be tuned based on system performance)
pub const MAX_CONCURRENT_UPLOADS: usize = 3;

// Maximum number of uploaded hashes to cache (prevents unbounded memory growth)
// Each hash is ~64 bytes, so 10,000 hashes = ~640KB
pub const MAX_UPLOADED_HASHES: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadItem {
    pub id: String,
    pub provider: String,
    pub project_name: String,
    pub file_path: PathBuf,
    pub file_name: String,
    pub queued_at: DateTime<Utc>,
    pub retry_count: u32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub file_hash: Option<String>, // SHA256 hash of file content for deduplication (v2 upload)
    pub file_size: u64,
    // Session timing information for historical uploads
    pub session_id: Option<String>,
    // In-memory content for parsed sessions (alternative to file_path)
    pub content: Option<String>,
    // Working directory for project metadata extraction
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStatus {
    pub pending: usize,
    pub processing: usize,
    pub failed: usize,
    pub recent_uploads: Vec<UploadItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItems {
    pub pending: Vec<UploadItem>,
    pub failed: Vec<UploadItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectUploadRequest {
    #[serde(rename = "projectName")]
    pub project_name: String,
    #[serde(rename = "gitRemoteUrl")]
    pub git_remote_url: Option<String>,
    pub cwd: String,
    #[serde(rename = "detectedProjectType")]
    pub detected_project_type: String,
}
