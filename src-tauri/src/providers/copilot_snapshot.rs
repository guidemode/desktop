use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::providers::copilot_parser::TimelineEntry;

/// Status of a snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotStatus {
    Active,
    Closed,
}

/// A single snapshot entry in metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotEntry {
    #[serde(rename = "snapshot_id")]
    pub snapshot_id: Uuid,
    #[serde(rename = "created_at")]
    pub created_at: String,
    #[serde(rename = "last_updated")]
    pub last_updated: String,
    #[serde(rename = "last_timeline_count")]
    pub last_timeline_count: usize,
    #[serde(rename = "last_source_file_size")]
    pub last_source_file_size: u64,
    pub status: SnapshotStatus,
}

/// Session entry tracking all snapshots for a source file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    #[serde(rename = "source_file")]
    pub source_file: String,
    #[serde(rename = "source_session_id")]
    pub source_session_id: String,
    #[serde(rename = "source_start_time")]
    pub source_start_time: String,
    pub snapshots: Vec<SnapshotEntry>,
    #[serde(rename = "active_snapshot_id")]
    pub active_snapshot_id: Uuid,
}

impl SessionEntry {
    pub fn get_active_snapshot(&self) -> Result<&SnapshotEntry, String> {
        self.snapshots
            .iter()
            .find(|s| s.snapshot_id == self.active_snapshot_id)
            .ok_or_else(|| "Active snapshot not found".to_string())
    }

    pub fn get_active_snapshot_mut(&mut self) -> Result<&mut SnapshotEntry, String> {
        self.snapshots
            .iter_mut()
            .find(|s| s.snapshot_id == self.active_snapshot_id)
            .ok_or_else(|| "Active snapshot not found".to_string())
    }

    pub fn close_active_snapshot(&mut self) -> Result<(), String> {
        let active = self.get_active_snapshot_mut()?;
        active.status = SnapshotStatus::Closed;
        Ok(())
    }

    pub fn add_snapshot(
        &mut self,
        snapshot_id: Uuid,
        timeline_count: usize,
        source_file_size: u64,
    ) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        self.snapshots.push(SnapshotEntry {
            snapshot_id,
            created_at: now.clone(),
            last_updated: now,
            last_timeline_count: timeline_count,
            last_source_file_size: source_file_size,
            status: SnapshotStatus::Active,
        });
        self.active_snapshot_id = snapshot_id;
        Ok(())
    }
}

/// Root metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub version: String,
    pub sessions: HashMap<String, SessionEntry>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            sessions: HashMap::new(),
        }
    }
}

/// Manager for Copilot snapshots
pub struct SnapshotManager {
    snapshot_dir: PathBuf,
    metadata_path: PathBuf,
}

impl SnapshotManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let home = dirs::home_dir().ok_or("Cannot find home directory")?;

        let base_dir = home.join(".guideai").join("providers").join("copilot");
        let snapshot_dir = base_dir.join("snapshots");
        let metadata_path = base_dir.join("metadata.json");

        // Create directories if they don't exist
        fs::create_dir_all(&snapshot_dir)?;

        // Set permissions to 700 on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&base_dir)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&base_dir, permissions)?;
        }

        Ok(Self {
            snapshot_dir,
            metadata_path,
        })
    }

    /// Load metadata with exclusive file lock
    pub fn load_metadata_locked(
        &self,
    ) -> Result<(Metadata, File), Box<dyn std::error::Error + Send + Sync>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&self.metadata_path)?;

        // Exclusive lock (blocks until available)
        file.lock_exclusive()?;

        let metadata = if file.metadata()?.len() > 0 {
            let reader = std::io::BufReader::new(&file);
            serde_json::from_reader(reader)?
        } else {
            Metadata::default()
        };

        Ok((metadata, file))
    }

    /// Save metadata atomically (while holding lock)
    pub fn save_metadata_atomic(
        &self,
        metadata: &Metadata,
        lock_file: File,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let temp_path = self.metadata_path.with_extension(".tmp");

        // Write to temp file
        let temp_file = File::create(&temp_path)?;
        let writer = BufWriter::new(temp_file);
        serde_json::to_writer_pretty(writer, metadata)?;

        // Sync to disk
        let file = OpenOptions::new().write(true).open(&temp_path)?;
        file.sync_all()?;

        // Atomic rename (still holding lock)
        fs::rename(temp_path, &self.metadata_path)?;

        // Release lock
        lock_file.unlock()?;

        Ok(())
    }

    /// Get snapshot file path
    pub fn get_snapshot_path(&self, snapshot_id: Uuid) -> PathBuf {
        self.snapshot_dir.join(format!("{}.jsonl", snapshot_id))
    }

    /// Create a new snapshot file with full timeline
    pub fn create_snapshot_file(
        &self,
        snapshot_id: Uuid,
        timeline: &[TimelineEntry],
        cwd: Option<&str>,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let snapshot_path = self.get_snapshot_path(snapshot_id);
        let file = File::create(&snapshot_path)?;
        let mut writer = BufWriter::new(file);

        for entry in timeline {
            // Flatten the timeline entry to JSONL format
            let mut json_obj = serde_json::Map::new();

            // Add timestamp if present
            if let Some(ref ts) = entry.timestamp {
                json_obj.insert("timestamp".to_string(), serde_json::json!(ts));
            }

            // Add cwd if provided
            if let Some(cwd_path) = cwd {
                json_obj.insert("cwd".to_string(), serde_json::json!(cwd_path));
            }

            // Add all other fields from the data
            if let serde_json::Value::Object(data_map) = &entry.data {
                for (key, value) in data_map {
                    json_obj.insert(key.clone(), value.clone());
                }
            }

            // Write as single JSON line
            writeln!(writer, "{}", serde_json::to_string(&json_obj)?)?;
        }

        writer.flush()?;
        drop(writer);

        // Sync to disk
        let file = OpenOptions::new().write(true).open(&snapshot_path)?;
        file.sync_all()?;

        Ok(snapshot_path)
    }

    /// Rewrite the entire snapshot file with the full timeline
    /// This ensures that any updates to existing entries (e.g., tool call results)
    /// are captured, and that cwd is added to every entry
    pub fn append_to_snapshot_file(
        &self,
        snapshot_id: Uuid,
        timeline: &[TimelineEntry],
        cwd: Option<&str>,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        // Just rewrite the entire file - files are not large
        self.create_snapshot_file(snapshot_id, timeline, cwd)
    }

    /// Detect if session has been truncated
    pub fn is_truncated(
        session: &SessionEntry,
        current_timeline_len: usize,
        current_file_size: u64,
    ) -> bool {
        let active = match session.get_active_snapshot() {
            Ok(s) => s,
            Err(_) => return false, // No active snapshot means first time
        };

        let last_count = active.last_timeline_count;
        let last_size = active.last_source_file_size;

        // Must have had content before
        if last_count == 0 {
            return false;
        }

        // Signal 1: Timeline length dropped significantly (>50%)
        let timeline_dropped = current_timeline_len < (last_count / 2);

        // Signal 2: File size dropped significantly (>50%)
        let size_dropped = last_size > 10000 && current_file_size < (last_size / 2);

        // Signal 3: Timeline is now empty
        let timeline_empty = current_timeline_len == 0;

        // Require at least 2 signals to avoid false positives
        let signal_count = [timeline_dropped, size_dropped, timeline_empty]
            .iter()
            .filter(|&&x| x)
            .count();

        signal_count >= 2
    }

    /// Get or create session entry for a source file
    pub fn get_or_create_session(
        &self,
        metadata: &mut Metadata,
        source_file: &Path,
        source_session_id: &str,
        source_start_time: &str,
        timeline: &[TimelineEntry],
        cwd: Option<&str>,
    ) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
        let file_name = source_file
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid filename")?;

        if let Some(session) = metadata.sessions.get(file_name) {
            return Ok(session.active_snapshot_id);
        }

        // First time - create initial snapshot with full timeline
        let snapshot_id = Uuid::new_v4();
        self.create_snapshot_file(snapshot_id, timeline, cwd)?;

        let now = Utc::now().to_rfc3339();
        let file_size = fs::metadata(source_file)?.len();

        let session = SessionEntry {
            source_file: file_name.to_string(),
            source_session_id: source_session_id.to_string(),
            source_start_time: source_start_time.to_string(),
            snapshots: vec![SnapshotEntry {
                snapshot_id,
                created_at: now.clone(),
                last_updated: now,
                last_timeline_count: timeline.len(),
                last_source_file_size: file_size,
                status: SnapshotStatus::Active,
            }],
            active_snapshot_id: snapshot_id,
        };

        metadata.sessions.insert(file_name.to_string(), session);

        Ok(snapshot_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_snapshot_creation() {
        let temp_dir = tempdir().unwrap();
        let manager = SnapshotManager {
            snapshot_dir: temp_dir.path().to_path_buf(),
            metadata_path: temp_dir.path().join("metadata.json"),
        };

        let timeline = vec![
            TimelineEntry {
                timestamp: Some("2025-01-01T10:00:00Z".to_string()),
                data: serde_json::json!({"type": "user", "text": "Hello"}),
            },
            TimelineEntry {
                timestamp: Some("2025-01-01T10:00:01Z".to_string()),
                data: serde_json::json!({"type": "assistant", "text": "Hi"}),
            },
        ];

        let snapshot_id = Uuid::new_v4();
        let path = manager
            .create_snapshot_file(snapshot_id, &timeline, Some("/test/project"))
            .unwrap();

        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().count(), 2);
    }

    #[test]
    fn test_truncation_detection() {
        let snapshot_id = Uuid::new_v4();
        let session = SessionEntry {
            source_file: "test.json".to_string(),
            source_session_id: "abc123".to_string(),
            source_start_time: "2025-01-01T10:00:00Z".to_string(),
            snapshots: vec![SnapshotEntry {
                snapshot_id,
                created_at: "2025-01-01T10:00:00Z".to_string(),
                last_updated: "2025-01-01T10:00:00Z".to_string(),
                last_timeline_count: 100,
                last_source_file_size: 50000,
                status: SnapshotStatus::Active,
            }],
            active_snapshot_id: snapshot_id,
        };

        // Signal 1 + 2: Timeline dropped from 100 to 10, size dropped from 50000 to 5000
        assert!(SnapshotManager::is_truncated(&session, 10, 5000));

        // Signal 2 + 3: Size dropped, timeline empty
        assert!(SnapshotManager::is_truncated(&session, 0, 5000));

        // Only 1 signal: timeline dropped but size similar
        assert!(!SnapshotManager::is_truncated(&session, 40, 48000));

        // Normal growth
        assert!(!SnapshotManager::is_truncated(&session, 105, 52000));
    }
}
