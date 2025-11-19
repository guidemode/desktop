/// Cursor-specific type definitions
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Session metadata from Cursor's meta table (key = "0")
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub agent_id: String,
    pub latest_root_blob_id: String,
    pub name: String,
    pub mode: String,
    pub created_at: i64, // Unix timestamp in milliseconds
    pub last_used_model: String,
}

/// A Cursor session with its database path and metadata
#[derive(Debug, Clone)]
pub struct CursorSession {
    /// Session UUID (directory name)
    pub session_id: String,

    /// Path to the store.db file
    pub db_path: PathBuf,

    /// Session metadata from the meta table
    pub metadata: SessionMetadata,

    /// Parent hash directory name (MD5 of CWD)
    /// Kept for compatibility but no longer actively used
    #[allow(dead_code)]
    pub hash: String,

    /// Current working directory (derived from projects directory)
    pub cwd: Option<String>,
}

impl CursorSession {
    /// Get a human-readable project name from the session
    /// Derives from CWD if available, otherwise uses session name
    pub fn project_name(&self) -> String {
        self.cwd
            .as_ref()
            .and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| self.metadata.name.clone())
    }

    /// Get created timestamp as DateTime
    #[allow(dead_code)] // Helper method, may be used in future
    pub fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp_millis(self.metadata.created_at)
            .unwrap_or_else(chrono::Utc::now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_metadata_deserialize() {
        let json = r#"{
            "agentId": "a562b4a7-31d2-45d6-9141-2be5c4edf3ef",
            "latestRootBlobId": "9f437c21a459ff88470d490a5736bc9acf4c93c34308b6c91f77dacce621fdff",
            "name": "Server Linter",
            "mode": "default",
            "createdAt": 1762058138859,
            "lastUsedModel": "default"
        }"#;

        let metadata: SessionMetadata = serde_json::from_str(json).unwrap();

        assert_eq!(metadata.agent_id, "a562b4a7-31d2-45d6-9141-2be5c4edf3ef");
        assert_eq!(metadata.name, "Server Linter");
        assert_eq!(metadata.mode, "default");
    }
}
