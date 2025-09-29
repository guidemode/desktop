use crate::config::GuideAIConfig;
use crate::logging::{log_error, log_info, log_warn};
use crate::providers::SessionInfo;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
    pub file_hash: Option<u64>, // Hash of file content for deduplication
    pub file_size: u64,
    // Session timing information for historical uploads
    pub session_id: Option<String>,
    // In-memory content for parsed sessions (alternative to file_path)
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStatus {
    pub pending: usize,
    pub processing: usize,
    pub failed: usize,
    pub recent_uploads: Vec<UploadItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadRequest {
    pub provider: String,
    #[serde(rename = "projectName")]
    pub project_name: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub content: String, // base64 encoded
}

#[derive(Debug, Clone)]
pub struct UploadQueue {
    queue: Arc<Mutex<VecDeque<UploadItem>>>,
    processing: Arc<Mutex<usize>>,
    failed_items: Arc<Mutex<Vec<UploadItem>>>,
    uploaded_hashes: Arc<Mutex<std::collections::HashSet<u64>>>, // Track uploaded file hashes
    is_running: Arc<Mutex<bool>>,
    config: Arc<Mutex<Option<GuideAIConfig>>>,
}

impl UploadQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            processing: Arc::new(Mutex::new(0)),
            failed_items: Arc::new(Mutex::new(Vec::new())),
            uploaded_hashes: Arc::new(Mutex::new(std::collections::HashSet::new())),
            is_running: Arc::new(Mutex::new(false)),
            config: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_config(&self, config: GuideAIConfig) {
        if let Ok(mut config_guard) = self.config.lock() {
            *config_guard = Some(config);
        }
    }

    pub fn add_item(&self, provider: &str, project_name: &str, file_path: PathBuf) -> Result<(), String> {
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Invalid file name")?
            .to_string();

        // Calculate file hash and size for deduplication
        let (file_hash, file_size) = Self::calculate_file_hash(&file_path)?;

        // Check if this file has already been uploaded
        if self.is_file_already_uploaded(file_hash) {
            log_info(provider, &format!("⚡ Skipping duplicate upload: {} (already uploaded)", file_name))
                .unwrap_or_default();
            return Ok(());
        }

        let item = UploadItem {
            id: Uuid::new_v4().to_string(),
            provider: provider.to_string(),
            project_name: project_name.to_string(),
            file_path: file_path.clone(),
            file_name,
            queued_at: Utc::now(),
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            file_hash: Some(file_hash),
            file_size,
            session_id: None,
            content: None,
        };

        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(item.clone());
        }

        Ok(())
    }

    pub fn add_historical_session(&self, session: &SessionInfo) -> Result<(), String> {
        // Handle sessions with in-memory content vs file-based sessions differently
        let (file_hash, file_size, content) = if let Some(ref content) = session.content {
            // For sessions with in-memory content (like OpenCode), hash the content directly
            let content_hash = {
                let mut hasher = DefaultHasher::new();
                content.hash(&mut hasher);
                hasher.finish()
            };
            let content_size = content.len() as u64;
            (content_hash, content_size, Some(content.clone()))
        } else {
            // For file-based sessions, calculate file hash
            let (file_hash, file_size) = Self::calculate_file_hash(&session.file_path)?;
            (file_hash, file_size, None)
        };

        // Check if this file has already been uploaded
        if self.is_file_already_uploaded(file_hash) {
            log_info(&session.provider, &format!("⚡ Skipping duplicate historical upload: {} (already uploaded)", session.file_name))
                .unwrap_or_default();
            return Ok(());
        }

        let item = UploadItem {
            id: Uuid::new_v4().to_string(),
            provider: session.provider.clone(),
            project_name: session.project_name.clone(),
            file_path: session.file_path.clone(),
            file_name: session.file_name.clone(),
            queued_at: Utc::now(),
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            file_hash: Some(file_hash),
            file_size,
            session_id: Some(session.session_id.clone()),
            content,
        };

        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(item.clone());
        }

        Ok(())
    }

    pub fn add_session_content(
        &self,
        provider: &str,
        project_name: &str,
        session_id: &str,
        content: String,
    ) -> Result<(), String> {
        // Calculate content hash for deduplication
        let content_hash = {
            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            hasher.finish()
        };

        let content_size = content.len() as u64;

        // Check if this content has already been uploaded
        if self.is_file_already_uploaded(content_hash) {
            log_info(provider, &format!("⚡ Skipping duplicate session upload: {} (already uploaded)", session_id))
                .unwrap_or_default();
            return Ok(());
        }

        let file_name = format!("{}.jsonl", session_id);

        let item = UploadItem {
            id: Uuid::new_v4().to_string(),
            provider: provider.to_string(),
            project_name: project_name.to_string(),
            file_path: PathBuf::from(&file_name), // Dummy path for in-memory content
            file_name,
            queued_at: Utc::now(),
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            file_hash: Some(content_hash),
            file_size: content_size,
            session_id: Some(session_id.to_string()),
            content: Some(content),
        };

        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(item.clone());
        }

        Ok(())
    }

    pub fn start_processing(&self) -> Result<(), String> {
        if let Ok(mut is_running) = self.is_running.lock() {
            if *is_running {
                return Ok(()); // Already running
            }
            *is_running = true;
        }

        let queue_clone = Arc::clone(&self.queue);
        let processing_clone = Arc::clone(&self.processing);
        let failed_items_clone = Arc::clone(&self.failed_items);
        let uploaded_hashes_clone = Arc::clone(&self.uploaded_hashes);
        let is_running_clone = Arc::clone(&self.is_running);
        let config_clone = Arc::clone(&self.config);

        // Spawn background thread for processing
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                log_info("upload-queue", "📤 Upload processor started").unwrap_or_default();

                loop {
                    // Check if we should continue running
                    {
                        if let Ok(is_running) = is_running_clone.lock() {
                            if !*is_running {
                                break;
                            }
                        }
                    }

                    // Process next item in queue
                    let item_to_process = {
                        if let Ok(mut queue) = queue_clone.lock() {
                            // Check for items ready to retry
                            if let Some(item) = Self::find_ready_item(&mut queue) {
                                Some(item)
                            } else {
                                queue.pop_front()
                            }
                        } else {
                            None
                        }
                    };

                    if let Some(mut item) = item_to_process {
                        // Increment processing counter
                        {
                            if let Ok(mut processing) = processing_clone.lock() {
                                *processing += 1;
                            }
                        }

                        // Get config for upload
                        let config = {
                            if let Ok(config_guard) = config_clone.lock() {
                                config_guard.clone()
                            } else {
                                None
                            }
                        };

                        // Log upload attempt
                        log_info(
                            &item.provider,
                            &format!("Uploading {} to server (attempt {})", item.file_name, item.retry_count + 1),
                        ).unwrap_or_default();

                        // Process the upload
                        match Self::process_upload_item(&item, config).await {
                            Ok(_) => {
                                // Mark file hash as uploaded to prevent future duplicates
                                if let Some(file_hash) = item.file_hash {
                                    if let Ok(mut uploaded_hashes) = uploaded_hashes_clone.lock() {
                                        uploaded_hashes.insert(file_hash);
                                    }
                                }

                                log_info(
                                    &item.provider,
                                    &format!("✓ Upload successful: {} (size: {} bytes)", item.file_name, item.file_size),
                                ).unwrap_or_default();
                            }
                            Err(e) => {
                                item.retry_count += 1;
                                item.last_error = Some(e.clone());

                                if item.retry_count <= 3 {
                                    // Calculate exponential backoff
                                    let delay_seconds = 2u64.pow(item.retry_count);
                                    item.next_retry_at = Some(
                                        Utc::now() + chrono::Duration::seconds(delay_seconds as i64)
                                    );

                                    // Put back in queue for retry
                                    if let Ok(mut queue) = queue_clone.lock() {
                                        queue.push_back(item.clone());
                                    }

                                    log_warn(
                                        &item.provider,
                                        &format!("⚠ Upload failed, retrying {} in {}s: {}", item.file_name, delay_seconds, e),
                                    ).unwrap_or_default();
                                } else {
                                    // Max retries exceeded, move to failed list
                                    if let Ok(mut failed) = failed_items_clone.lock() {
                                        failed.push(item.clone());
                                    }

                                    log_error(
                                        &item.provider,
                                        &format!("✗ Upload failed permanently: {} (after {} attempts)", item.file_name, item.retry_count),
                                    ).unwrap_or_default();
                                }
                            }
                        }

                        // Decrement processing counter
                        {
                            if let Ok(mut processing) = processing_clone.lock() {
                                *processing = processing.saturating_sub(1);
                            }
                        }
                    } else {
                        // No items to process, sleep for a bit
                        sleep(Duration::from_secs(5)).await;
                    }
                }

                log_info("upload-queue", "📤 Upload processor stopped").unwrap_or_default();
            });
        });

        Ok(())
    }

    #[allow(dead_code)]
    pub fn stop_processing(&self) {
        if let Ok(mut is_running) = self.is_running.lock() {
            *is_running = false;
        }
    }

    pub fn get_status(&self) -> UploadStatus {
        let pending = if let Ok(queue) = self.queue.lock() {
            queue.len()
        } else {
            0
        };

        let processing = if let Ok(processing) = self.processing.lock() {
            *processing
        } else {
            0
        };

        let failed = if let Ok(failed) = self.failed_items.lock() {
            failed.len()
        } else {
            0
        };

        // Get recent uploads (last 10)
        let recent_uploads = if let Ok(failed) = self.failed_items.lock() {
            failed.iter().rev().take(10).cloned().collect()
        } else {
            Vec::new()
        };

        UploadStatus {
            pending,
            processing,
            failed,
            recent_uploads,
        }
    }

    fn calculate_file_hash(file_path: &PathBuf) -> Result<(u64, u64), String> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(file_path)
            .map_err(|e| format!("Failed to open file for hashing: {}", e))?;

        let mut hasher = DefaultHasher::new();
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)
            .map_err(|e| format!("Failed to read file for hashing: {}", e))?;

        let file_size = buffer.len() as u64;

        // Hash the file content
        buffer.hash(&mut hasher);
        let file_hash = hasher.finish();

        Ok((file_hash, file_size))
    }

    fn is_file_already_uploaded(&self, file_hash: u64) -> bool {
        if let Ok(uploaded_hashes) = self.uploaded_hashes.lock() {
            uploaded_hashes.contains(&file_hash)
        } else {
            false
        }
    }

    fn find_ready_item(queue: &mut VecDeque<UploadItem>) -> Option<UploadItem> {
        let now = Utc::now();

        for (index, item) in queue.iter().enumerate() {
            if let Some(retry_at) = item.next_retry_at {
                if now >= retry_at {
                    return queue.remove(index);
                }
            }
        }
        None
    }

    async fn process_upload_item(
        item: &UploadItem,
        config: Option<GuideAIConfig>,
    ) -> Result<(), String> {
        let config = config.ok_or("No configuration available")?;

        let api_key = config.api_key.ok_or("No API key configured")?;
        let server_url = config.server_url.ok_or("No server URL configured")?;
        let _tenant_id = config.tenant_id.ok_or("No tenant ID configured")?;

        // Get content - either from memory or from file
        let encoded_content = if let Some(ref content) = item.content {
            // Use in-memory content (already text, encode as base64)
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(content.as_bytes())
        } else {
            // Read file content
            let file_content = std::fs::read(&item.file_path)
                .map_err(|e| format!("Failed to read file: {}", e))?;

            // Encode to base64
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&file_content)
        };

        // Extract session ID - prefer from item metadata, fallback to filename
        let session_id = item.session_id.clone().unwrap_or_else(|| {
            item.file_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown-session")
                .to_string()
        });

        // Prepare upload request
        let upload_request = UploadRequest {
            provider: item.provider.clone(),
            project_name: item.project_name.clone(),
            session_id,
            file_name: item.file_name.clone(),
            file_path: item.file_path.to_string_lossy().to_string(),
            content: encoded_content,
        };

        // Make HTTP request to server
        let client = reqwest::Client::new();
        let url = format!("{}/api/agent-sessions/upload", server_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&upload_request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            Err(format!("Upload failed with status {}: {}", status, error_text))
        }
    }

    pub fn clear_failed(&self) {
        if let Ok(mut failed) = self.failed_items.lock() {
            failed.clear();
        }
    }

    pub fn retry_failed(&self) {
        if let Ok(mut failed) = self.failed_items.lock() {
            if let Ok(mut queue) = self.queue.lock() {
                // Move all failed items back to queue, reset retry count
                for mut item in failed.drain(..) {
                    item.retry_count = 0;
                    item.next_retry_at = None;
                    item.last_error = None;
                    queue.push_back(item);
                }
            }
        }
    }

    pub fn clear_uploaded_hashes(&self) {
        if let Ok(mut uploaded_hashes) = self.uploaded_hashes.lock() {
            uploaded_hashes.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

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
        let queue = UploadQueue::new();
        let temp_file = NamedTempFile::new().unwrap();

        let result = queue.add_item("claude-code", "test-project", temp_file.path().to_path_buf());
        assert!(result.is_ok());

        let status = queue.get_status();
        assert_eq!(status.pending, 1);
    }

    #[test]
    fn test_add_session_content() {
        let queue = UploadQueue::new();
        let content = r#"{"sessionId":"test-session","timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]}}"#;

        let result = queue.add_session_content(
            "opencode",
            "test-project",
            "test-session",
            content.to_string(),
        );
        assert!(result.is_ok());

        let status = queue.get_status();
        assert_eq!(status.pending, 1);
    }
}