// Module declarations
mod compression;
mod hashing;
mod processor;
mod queue_manager;
mod types;
mod upload;
mod validation;

// Re-export types and constants from submodules
pub use types::*;

use crate::config::GuideModeConfig;
use crate::project_metadata::ProjectMetadata;
use crate::providers::SessionInfo;
use indexmap::IndexSet;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

// Import validation function and PathBuf for tests only
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use validation::validate_jsonl_timestamps;

// Types, constants, and structs are now imported from the types module via `pub use types::*;`

#[derive(Clone)]
pub struct UploadQueue {
    queue: Arc<Mutex<VecDeque<UploadItem>>>,
    processing: Arc<Mutex<usize>>,
    failed_items: Arc<Mutex<Vec<UploadItem>>>,
    uploaded_hashes: Arc<Mutex<IndexSet<String>>>, // Track uploaded file hashes (SHA256) with insertion order
    is_running: Arc<Mutex<bool>>,
    config: Arc<Mutex<Option<GuideModeConfig>>>,
    app_handle: Arc<Mutex<Option<tauri::AppHandle>>>,
    upload_semaphore: Arc<Semaphore>, // Limit concurrent uploads
}

impl std::fmt::Debug for UploadQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadQueue")
            .field("queue", &"<queued items>")
            .field("processing", &self.processing)
            .field("failed_items", &"<failed items>")
            .field("uploaded_hashes", &"<hash set>")
            .field("is_running", &self.is_running)
            .field("config", &"<config>")
            .field("app_handle", &"<app handle>")
            .field("upload_semaphore", &"<semaphore>")
            .finish()
    }
}

impl Default for UploadQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl UploadQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            processing: Arc::new(Mutex::new(0)),
            failed_items: Arc::new(Mutex::new(Vec::new())),
            uploaded_hashes: Arc::new(Mutex::new(IndexSet::new())),
            is_running: Arc::new(Mutex::new(false)),
            config: Arc::new(Mutex::new(None)),
            app_handle: Arc::new(Mutex::new(None)),
            upload_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_UPLOADS)),
        }
    }

    pub fn set_config(&self, config: GuideModeConfig) {
        if let Ok(mut config_guard) = self.config.lock() {
            *config_guard = Some(config);
        }
    }

    pub fn set_app_handle(&self, app_handle: tauri::AppHandle) {
        if let Ok(mut handle_guard) = self.app_handle.lock() {
            *handle_guard = Some(app_handle);
        }
    }

    #[cfg(test)]
    pub fn add_item(
        &self,
        provider: &str,
        project_name: &str,
        file_path: PathBuf,
    ) -> Result<(), String> {
        queue_manager::add_item(
            &self.queue,
            &self.uploaded_hashes,
            provider,
            project_name,
            file_path,
        )
    }

    pub fn add_historical_session(&self, session: &SessionInfo) -> Result<(), String> {
        queue_manager::add_historical_session(
            &self.queue,
            &self.uploaded_hashes,
            &self.config,
            session,
        )
    }

    #[cfg(test)]
    pub fn add_session_content(
        &self,
        provider: &str,
        project_name: &str,
        session_id: &str,
        content: String,
    ) -> Result<(), String> {
        queue_manager::add_session_content(
            &self.queue,
            &self.uploaded_hashes,
            provider,
            project_name,
            session_id,
            content,
        )
    }

    pub fn start_processing(&self) -> Result<(), String> {
        // Create processor and delegate to it
        let processor = processor::UploadProcessor::new(
            Arc::clone(&self.queue),
            Arc::clone(&self.processing),
            Arc::clone(&self.failed_items),
            Arc::clone(&self.uploaded_hashes),
            Arc::clone(&self.is_running),
            Arc::clone(&self.config),
            Arc::clone(&self.app_handle),
            Arc::clone(&self.upload_semaphore),
        );

        processor.start()
    }

    pub fn get_status(&self) -> UploadStatus {
        queue_manager::get_status(&self.processing)
    }

    #[allow(dead_code)]
    fn is_file_already_uploaded(&self, file_hash: &str) -> bool {
        queue_manager::is_file_already_uploaded(&self.uploaded_hashes, file_hash)
    }

    pub fn clear_failed(&self) {
        queue_manager::clear_failed();
    }

    pub fn retry_failed(&self) {
        queue_manager::retry_failed();
    }

    pub fn clear_uploaded_hashes(&self) {
        if let Ok(mut uploaded_hashes) = self.uploaded_hashes.lock() {
            uploaded_hashes.clear();
        }
    }

    pub fn get_all_items(&self) -> QueueItems {
        queue_manager::get_all_items()
    }

    pub fn remove_item(&self, item_id: &str) -> Result<(), String> {
        queue_manager::remove_item(item_id)
    }

    pub fn retry_item(&self, item_id: &str) -> Result<(), String> {
        queue_manager::retry_item(item_id)
    }

    /// Upload project metadata to the server
    ///
    /// **DEPRECATED**: Use embedded projectMetadata in upload payloads instead
    #[allow(dead_code)]
    #[allow(deprecated)]
    pub async fn upload_project_metadata(&self, metadata: &ProjectMetadata) -> Result<(), String> {
        let config = if let Ok(config_guard) = self.config.lock() {
            config_guard.clone()
        } else {
            None
        };

        upload::upload_project_metadata_static(metadata, config).await
    }

    /// Check if a project exists on the server (GET request)
    #[allow(dead_code)]
    pub async fn check_project_exists(&self, project_name: &str) -> Result<bool, String> {
        let config = if let Ok(config_guard) = self.config.lock() {
            config_guard.clone()
        } else {
            None
        };

        let config = config.ok_or("No configuration available")?;
        let api_key = config.api_key.ok_or("No API key configured")?;
        let server_url = config.server_url.ok_or("No server URL configured")?;

        // Make GET request to check if project exists
        let client = reqwest::Client::new();
        let url = format!("{}/api/projects/{}", server_url, project_name);

        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        Ok(response.status().is_success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upload_queue_creation() {
        let queue = UploadQueue::new();
        let status = queue.get_status();

        assert_eq!(status.pending, 0);
        assert_eq!(status.processing, 0);
        assert_eq!(status.failed, 0);
    }

    #[test]
    fn test_add_item() {
        use std::fs;

        let queue = UploadQueue::new();

        // Create temp file in allowed directory (~/.guidemode/test)
        let home = dirs::home_dir().unwrap();
        let test_dir = home.join(".guidemode").join("test");
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test_session.jsonl");
        let jsonl_content =
            r#"{"timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":"test"}"#;
        fs::write(&test_file, jsonl_content).unwrap();

        let result = queue.add_item("claude-code", "test-project", test_file.clone());

        // Clean up
        fs::remove_file(&test_file).ok();

        assert!(result.is_ok(), "add_item failed: {:?}", result.err());

        // Check in-memory queue directly since get_status() reads from database
        let queue_len = queue.queue.lock().unwrap().len();
        assert_eq!(queue_len, 1, "Expected 1 item in queue, got {}", queue_len);
    }

    #[test]
    fn test_add_session_content() {
        let queue = UploadQueue::new();
        let content = r#"{"timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#;

        // Manually test validation
        let (is_valid, error) = validate_jsonl_timestamps(content);
        assert!(is_valid, "Validation failed: {:?}", error);

        let result = queue.add_session_content(
            "opencode",
            "test-project",
            "test-session",
            content.to_string(),
        );
        assert!(
            result.is_ok(),
            "add_session_content failed: {:?}",
            result.err()
        );

        // Check in-memory queue directly since get_status() reads from database
        let queue_len = queue.queue.lock().unwrap().len();
        assert_eq!(queue_len, 1, "Expected 1 item in queue, got {}", queue_len);
    }

    #[test]
    fn test_validate_jsonl_timestamps() {
        // Valid JSONL with timestamp
        let valid_content =
            r#"{"timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":"test"}"#;
        let (is_valid, error) = validate_jsonl_timestamps(valid_content);
        assert!(is_valid);
        assert!(error.is_none());

        // Invalid JSONL without timestamp
        let invalid_content = r#"{"type":"user","message":"test"}"#;
        let (is_valid, error) = validate_jsonl_timestamps(invalid_content);
        assert!(!is_valid);
        assert!(error.is_some());

        // Empty content
        let (is_valid, error) = validate_jsonl_timestamps("");
        assert!(!is_valid);
        assert!(error.is_some());

        // Multiple lines with at least one timestamp
        let mixed_content = r#"{"type":"user","message":"test"}
{"timestamp":"2025-01-01T10:00:00.000Z","type":"assistant","message":"response"}"#;
        let (is_valid, error) = validate_jsonl_timestamps(mixed_content);
        assert!(is_valid);
        assert!(error.is_none());
    }

    #[test]
    fn test_add_item_without_timestamps() {
        use std::fs;

        let queue = UploadQueue::new();

        // Create temp file in allowed directory (~/.guidemode/test)
        let home = dirs::home_dir().unwrap();
        let test_dir = home.join(".guidemode").join("test");
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test_session_no_timestamps.jsonl");
        let jsonl_content = r#"{"type":"user","message":"test"}"#;
        fs::write(&test_file, jsonl_content).unwrap();

        let result = queue.add_item("claude-code", "test-project", test_file.clone());

        // Clean up
        fs::remove_file(&test_file).ok();

        assert!(result.is_ok(), "add_item failed: {:?}", result.err());

        // Should not be added to queue due to missing timestamps
        let status = queue.get_status();
        assert_eq!(status.pending, 0);
    }
}
