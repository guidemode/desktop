use crate::config::GuideAIConfig;
use crate::database::{get_unsynced_sessions, mark_session_sync_failed, mark_session_synced};
use crate::logging::{log_debug, log_error, log_info, log_warn};
use crate::project_metadata::ProjectMetadata;
use crate::providers::SessionInfo;
use crate::validation::{validate_session_file, MAX_SESSION_FILE_SIZE};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;
use tokio::time::sleep;
use uuid::Uuid;

// Database polling interval (10 seconds by default, configurable later)
const DB_POLL_INTERVAL_SECS: u64 = 10;

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

#[derive(Clone)]
pub struct UploadQueue {
    queue: Arc<Mutex<VecDeque<UploadItem>>>,
    processing: Arc<Mutex<usize>>,
    failed_items: Arc<Mutex<Vec<UploadItem>>>,
    uploaded_hashes: Arc<Mutex<std::collections::HashSet<u64>>>, // Track uploaded file hashes
    is_running: Arc<Mutex<bool>>,
    config: Arc<Mutex<Option<GuideAIConfig>>>,
    app_handle: Arc<Mutex<Option<tauri::AppHandle>>>,
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
            .finish()
    }
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
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_config(&self, config: GuideAIConfig) {
        if let Ok(mut config_guard) = self.config.lock() {
            *config_guard = Some(config);
        }
    }

    pub fn set_app_handle(&self, app_handle: tauri::AppHandle) {
        if let Ok(mut handle_guard) = self.app_handle.lock() {
            *handle_guard = Some(app_handle);
        }
    }

    /// Validate that JSONL content contains at least one entry with a timestamp field
    fn validate_jsonl_timestamps(content: &str) -> (bool, Option<String>) {
        let lines: Vec<&str> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();

        if lines.is_empty() {
            return (
                false,
                Some("File is empty or contains only whitespace".to_string()),
            );
        }

        let mut has_valid_json = false;
        let mut parse_errors = 0;

        // Check if at least one line has a timestamp field
        for (index, line) in lines.iter().enumerate() {
            if let Ok(entry) = serde_json::from_str::<Value>(line) {
                has_valid_json = true;
                // Look for timestamp field (common across providers)
                if entry.get("timestamp").is_some() {
                    return (true, None);
                }
            } else {
                parse_errors += 1;
                if index < 3 {
                    // Log first few parse errors for debugging
                    log_warn(
                        "upload-queue",
                        &format!(
                            "  Line {} failed to parse as JSON: {}",
                            index + 1,
                            &line[..line.len().min(100)]
                        ),
                    )
                    .unwrap_or_default();
                }
            }
        }

        if !has_valid_json {
            return (
                false,
                Some(format!(
                    "No valid JSON lines found ({} parse errors)",
                    parse_errors
                )),
            );
        }

        (
            false,
            Some(format!(
                "No timestamp field found in any of {} lines ({} valid JSON entries)",
                lines.len(),
                lines.len() - parse_errors
            )),
        )
    }

    #[cfg(test)]
    pub fn add_item(
        &self,
        provider: &str,
        project_name: &str,
        file_path: PathBuf,
    ) -> Result<(), String> {
        // Validate path and check file size
        let (validated_path, file_size) =
            validate_session_file(&file_path).map_err(|e| e.to_string())?;

        let file_name = validated_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Invalid file name")?
            .to_string();

        // Read and validate file content
        let file_content = std::fs::read_to_string(&validated_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let (is_valid, validation_error) = Self::validate_jsonl_timestamps(&file_content);
        if !is_valid {
            let reason =
                validation_error.unwrap_or_else(|| "no valid timestamps found".to_string());
            log_warn(
                "upload-queue",
                &format!("⚠ Skipping upload: {} ({})", file_name, reason),
            )
            .unwrap_or_default();
            return Ok(());
        }

        // Calculate file hash for deduplication
        let (file_hash, _) = Self::calculate_file_hash(&validated_path)?;

        // Check if this file has already been uploaded
        if self.is_file_already_uploaded(file_hash) {
            log_info(
                "upload-queue",
                &format!(
                    "⚡ Skipping duplicate upload: {} (already uploaded)",
                    file_name
                ),
            )
            .unwrap_or_default();
            return Ok(());
        }

        let item = UploadItem {
            id: Uuid::new_v4().to_string(),
            provider: provider.to_string(),
            project_name: project_name.to_string(),
            file_path: validated_path,
            file_name,
            queued_at: Utc::now(),
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            file_hash: Some(file_hash),
            file_size,
            session_id: None,
            content: None,
            cwd: None,
        };

        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(item.clone());
        }

        Ok(())
    }

    pub fn add_historical_session(&self, session: &SessionInfo) -> Result<(), String> {
        // Handle sessions with in-memory content vs file-based sessions differently
        let (file_hash, file_size, content) = if let Some(ref content) = session.content {
            // For sessions with in-memory content (like OpenCode), validate size and timestamps
            let content_size = content.len() as u64;

            // Check size limit
            if content_size > MAX_SESSION_FILE_SIZE {
                let reason = format!(
                    "content size ({} bytes) exceeds maximum ({} bytes)",
                    content_size, MAX_SESSION_FILE_SIZE
                );
                log_warn(
                    "upload-queue",
                    &format!(
                        "⚠ Skipping historical upload: {} ({})",
                        session.file_name, reason
                    ),
                )
                .unwrap_or_default();
                return Ok(());
            }

            let (is_valid, validation_error) = Self::validate_jsonl_timestamps(content);
            if !is_valid {
                let reason =
                    validation_error.unwrap_or_else(|| "no valid timestamps found".to_string());
                log_warn(
                    "upload-queue",
                    &format!(
                        "⚠ Skipping historical upload: {} ({})",
                        session.file_name, reason
                    ),
                )
                .unwrap_or_default();
                return Ok(());
            }

            let content_hash = {
                let mut hasher = DefaultHasher::new();
                content.hash(&mut hasher);
                hasher.finish()
            };
            (content_hash, content_size, Some(content.clone()))
        } else {
            // For file-based sessions, validate path and check file size
            let (validated_path, file_size) =
                validate_session_file(&session.file_path).map_err(|e| e.to_string())?;

            // Read and validate content
            let file_content = std::fs::read_to_string(&validated_path)
                .map_err(|e| format!("Failed to read file: {}", e))?;

            let (is_valid, validation_error) = Self::validate_jsonl_timestamps(&file_content);
            if !is_valid {
                let reason =
                    validation_error.unwrap_or_else(|| "no valid timestamps found".to_string());
                log_warn(
                    "upload-queue",
                    &format!(
                        "⚠ Skipping historical upload: {} ({})",
                        session.file_name, reason
                    ),
                )
                .unwrap_or_default();
                return Ok(());
            }

            let (file_hash, _) = Self::calculate_file_hash(&validated_path)?;
            (file_hash, file_size, None)
        };

        // Check if this file has already been uploaded
        if self.is_file_already_uploaded(file_hash) {
            log_info(
                "upload-queue",
                &format!(
                    "⚡ Skipping duplicate historical upload: {} (already uploaded)",
                    session.file_name
                ),
            )
            .unwrap_or_default();
            return Ok(());
        }

        // Extract and upload project metadata if CWD is available, and derive real project name
        let real_project_name = if let Some(ref cwd) = session.cwd {
            log_info("upload-queue", &format!("📁 Extracting project metadata from CWD: {} (for session {}, Claude folder: {})", cwd, session.session_id, session.project_name))
                .unwrap_or_default();

            use crate::project_metadata::extract_project_metadata;
            match extract_project_metadata(cwd) {
                Ok(metadata) => {
                    log_info("upload-queue", &format!("✓ Extracted project metadata: {} (type: {}, git: {}) - will use this as project name for session upload",
                        metadata.project_name,
                        metadata.detected_project_type,
                        metadata.git_remote_url.as_deref().unwrap_or("none")
                    )).unwrap_or_default();

                    // Clone necessary data for the async task
                    let config_clone = if let Ok(config_guard) = self.config.lock() {
                        config_guard.clone()
                    } else {
                        None
                    };

                    let metadata_clone = metadata.clone();
                    let session_id = session.session_id.clone();

                    // Spawn async task to upload project metadata
                    tokio::spawn(async move {
                        log_info(
                            "upload-queue",
                            &format!(
                                "📦 Uploading project metadata for {} (session: {})",
                                metadata_clone.project_name, session_id
                            ),
                        )
                        .unwrap_or_default();

                        if let Err(e) =
                            Self::upload_project_metadata_static(&metadata_clone, config_clone)
                                .await
                        {
                            log_warn(
                                "upload-queue",
                                &format!(
                                    "⚠ Failed to upload project metadata for {}: {}",
                                    metadata_clone.project_name, e
                                ),
                            )
                            .unwrap_or_default();
                        } else {
                            log_info(
                                "upload-queue",
                                &format!(
                                    "✓ Uploaded project metadata for {}",
                                    metadata_clone.project_name
                                ),
                            )
                            .unwrap_or_default();
                        }
                    });

                    // Use the derived project name for the session upload
                    Some(metadata.project_name)
                }
                Err(e) => {
                    log_warn("upload-queue", &format!("⚠ Could not extract project metadata from {} (session {}): {} - using Claude folder name instead", cwd, session.session_id, e))
                        .unwrap_or_default();
                    None
                }
            }
        } else {
            log_warn("upload-queue", &format!("⚠ No CWD available for session {} - cannot extract project metadata, using Claude folder name", session.session_id))
                .unwrap_or_default();
            None
        };

        // Use the real project name if available, otherwise fall back to Claude folder name
        let project_name_for_upload =
            real_project_name.unwrap_or_else(|| session.project_name.clone());

        log_info(
            "upload-queue",
            &format!(
                "📝 Creating upload item for session {} with project name: {} (Claude folder: {})",
                session.session_id, project_name_for_upload, session.project_name
            ),
        )
        .unwrap_or_default();

        let item = UploadItem {
            id: Uuid::new_v4().to_string(),
            provider: session.provider.clone(),
            project_name: project_name_for_upload,
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
            cwd: session.cwd.clone(),
        };

        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(item.clone());
        }

        Ok(())
    }

    #[cfg(test)]
    pub fn add_session_content(
        &self,
        provider: &str,
        project_name: &str,
        session_id: &str,
        content: String,
    ) -> Result<(), String> {
        // Validate content before adding to queue
        let (is_valid, validation_error) = Self::validate_jsonl_timestamps(&content);
        if !is_valid {
            let reason =
                validation_error.unwrap_or_else(|| "no valid timestamps found".to_string());
            log_warn(
                "upload-queue",
                &format!("⚠ Skipping session upload: {} ({})", session_id, reason),
            )
            .unwrap_or_default();
            return Ok(());
        }

        // Calculate content hash for deduplication
        let content_hash = {
            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            hasher.finish()
        };

        let content_size = content.len() as u64;

        // Check if this content has already been uploaded
        if self.is_file_already_uploaded(content_hash) {
            log_info(
                "upload-queue",
                &format!(
                    "⚡ Skipping duplicate session upload: {} (already uploaded)",
                    session_id
                ),
            )
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
            cwd: None,
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
        let app_handle_clone = Arc::clone(&self.app_handle);

        // Use Tauri's async runtime instead of spawning a new thread + runtime
        // This is more efficient and avoids creating multiple Tokio runtimes
        tauri::async_runtime::spawn(async move {
            log_info("upload-queue", "📤 Upload processor started").unwrap_or_default();

            let mut last_db_poll = Utc::now();

            loop {
                    // Check if we should continue running
                    {
                        if let Ok(is_running) = is_running_clone.lock() {
                            if !*is_running {
                                break;
                            }
                        }
                    }

                    // Check if we have valid auth before polling/processing
                    let has_valid_auth = {
                        if let Ok(config_guard) = config_clone.lock() {
                            if let Some(ref cfg) = *config_guard {
                                cfg.api_key.is_some() && cfg.server_url.is_some() && cfg.tenant_id.is_some()
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    // Only poll database if we have valid auth
                    let now = Utc::now();
                    if has_valid_auth && (now - last_db_poll).num_seconds() >= DB_POLL_INTERVAL_SECS as i64 {
                        match get_unsynced_sessions() {
                            Ok(unsynced_sessions) => {
                                if !unsynced_sessions.is_empty() {
                                    log_info(
                                        "upload-queue",
                                        &format!("📊 Found {} unsynced sessions in database", unsynced_sessions.len()),
                                    ).unwrap_or_default();

                                    // Add unsynced sessions to queue
                                    if let Ok(mut queue) = queue_clone.lock() {
                                        for session in unsynced_sessions {
                                            // Check if already in queue to avoid duplicates
                                            let already_queued = queue.iter().any(|item| {
                                                item.session_id.as_ref() == Some(&session.session_id)
                                            });

                                            if !already_queued {
                                                let item = UploadItem {
                                                    id: session.id.clone(),
                                                    provider: session.provider.clone(),
                                                    project_name: session.project_name.clone(),
                                                    file_path: PathBuf::from(&session.file_path),
                                                    file_name: session.file_name.clone(),
                                                    queued_at: now,
                                                    retry_count: 0,
                                                    next_retry_at: None,
                                                    last_error: None,
                                                    file_hash: None,
                                                    file_size: session.file_size as u64,
                                                    session_id: Some(session.session_id.clone()),
                                                    content: None,
                                                    cwd: session.cwd.clone(),
                                                };
                                                queue.push_back(item);
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log_error(
                                    "upload-queue",
                                    &format!("Failed to poll database for unsynced sessions: {}", e),
                                ).unwrap_or_default();
                            }
                        }
                        last_db_poll = now;
                    }

                    // Process next item in queue (only if we have valid auth)
                    let item_to_process = if has_valid_auth {
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
                    } else {
                        None
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
                            "upload-queue",
                            &format!("📤 Uploading {} to server (attempt {})", item.file_name, item.retry_count + 1),
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

                                // Mark session as synced in database
                                if let Some(ref session_id) = item.session_id {
                                    if let Err(e) = mark_session_synced(session_id, None) {
                                        log_error(
                                            "upload-queue",
                                            &format!("Failed to mark session {} as synced: {}", session_id, e),
                                        ).unwrap_or_default();
                                    } else {
                                        // Emit event to frontend to refresh session list
                                        if let Ok(app_handle_guard) = app_handle_clone.lock() {
                                            if let Some(ref app_handle) = *app_handle_guard {
                                                let _ = app_handle.emit("session-synced", session_id.clone());
                                            }
                                        }
                                    }
                                }

                                log_info(
                                    "upload-queue",
                                    &format!("✓ Upload successful: {} (size: {} bytes)", item.file_name, item.file_size),
                                ).unwrap_or_default();
                            }
                            Err(e) => {
                                item.last_error = Some(e.clone());

                                // Check if this is a 400 error (invalid input) - don't retry these
                                let is_client_error = e.contains("status 400") || e.contains("Bad Request");

                                if is_client_error {
                                    // 400 errors indicate invalid input - don't retry
                                    if let Ok(mut failed) = failed_items_clone.lock() {
                                        failed.push(item.clone());
                                    }

                                    // Mark session as sync failed in database
                                    if let Some(ref session_id) = item.session_id {
                                        if let Err(db_err) = mark_session_sync_failed(session_id, &e) {
                                            log_error(
                                                "upload-queue",
                                                &format!("Failed to mark session {} as sync failed: {}", session_id, db_err),
                                            ).unwrap_or_default();
                                        } else {
                                            // Emit event to frontend to refresh session list
                                            if let Ok(app_handle_guard) = app_handle_clone.lock() {
                                                if let Some(ref app_handle) = *app_handle_guard {
                                                    let _ = app_handle.emit("session-sync-failed", session_id.clone());
                                                }
                                            }
                                        }
                                    }

                                    log_error(
                                        "upload-queue",
                                        &format!("✗ Upload failed (invalid input, will not retry): {} - Error: {}", item.file_name, e),
                                    ).unwrap_or_default();
                                } else {
                                    // Network or server errors - retry with backoff
                                    item.retry_count += 1;

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
                                            "upload-queue",
                                            &format!("⚠ Upload failed, retrying {} in {}s: {}", item.file_name, delay_seconds, e),
                                        ).unwrap_or_default();
                                    } else {
                                        // Max retries exceeded, move to failed list
                                        if let Ok(mut failed) = failed_items_clone.lock() {
                                            failed.push(item.clone());
                                        }

                                        // Mark session as sync failed in database after all retries exhausted
                                        if let Some(ref session_id) = item.session_id {
                                            if let Err(db_err) = mark_session_sync_failed(session_id, &e) {
                                                log_error(
                                                    "upload-queue",
                                                    &format!("Failed to mark session {} as sync failed: {}", session_id, db_err),
                                                ).unwrap_or_default();
                                            } else {
                                                // Emit event to frontend to refresh session list
                                                if let Ok(app_handle_guard) = app_handle_clone.lock() {
                                                    if let Some(ref app_handle) = *app_handle_guard {
                                                        let _ = app_handle.emit("session-sync-failed", session_id.clone());
                                                    }
                                                }
                                            }
                                        }

                                        log_error(
                                            "upload-queue",
                                            &format!("✗ Upload failed permanently: {} (after {} attempts)", item.file_name, item.retry_count),
                                        ).unwrap_or_default();
                                    }
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

        Ok(())
    }

    pub fn get_status(&self) -> UploadStatus {
        use crate::database::{get_failed_sessions, get_upload_stats};

        // Get real-time stats from database instead of in-memory queue
        let db_stats = get_upload_stats().unwrap_or(crate::database::UploadStats {
            pending: 0,
            synced: 0,
            total: 0,
        });

        let processing = if let Ok(processing) = self.processing.lock() {
            *processing
        } else {
            0
        };

        // Get failed count from database
        let failed = get_failed_sessions()
            .map(|sessions| sessions.len())
            .unwrap_or(0);

        // Get recent uploads (last 10) - for now, empty since we're not tracking this
        let recent_uploads = Vec::new();

        UploadStatus {
            pending: db_stats.pending, // Real-time from database
            processing,
            failed,
            recent_uploads,
        }
    }

    fn calculate_file_hash(file_path: &PathBuf) -> Result<(u64, u64), String> {
        use std::fs::File;
        use std::io::Read;

        let mut file =
            File::open(file_path).map_err(|e| format!("Failed to open file for hashing: {}", e))?;

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

        // Check provider sync mode before uploading
        use crate::config::load_provider_config;
        let provider_config = load_provider_config(&item.provider)
            .map_err(|e| format!("Failed to load provider config: {}", e))?;

        // Route to appropriate upload function based on sync mode
        match provider_config.sync_mode.as_str() {
            "Metrics Only" => {
                // Metrics-only sync: upload session metadata and metrics without JSONL
                Self::upload_metrics_only(item, config.clone()).await
            }
            "Transcript and Metrics" => {
                // Full sync: upload JSONL + metrics
                Self::upload_transcript_and_metrics(item, config.clone()).await
            }
            _ => {
                Err(format!(
                    "Sync mode is '{}', skipping upload (expected 'Metrics Only' or 'Transcript and Metrics')",
                    provider_config.sync_mode
                ))
            }
        }
    }

    /// Upload session metadata and metrics only (no JSONL transcript)
    async fn upload_metrics_only(
        item: &UploadItem,
        config: GuideAIConfig,
    ) -> Result<(), String> {
        use crate::database::{get_full_session_by_id, get_session_metrics, get_session_rating};

        let api_key = config.api_key.clone().ok_or("No API key configured")?;
        let server_url = config
            .server_url
            .clone()
            .ok_or("No server URL configured")?;

        // Get session ID
        let session_id = item.session_id.as_ref()
            .ok_or("Session ID required for metrics-only sync")?;

        // Fetch full session data from database
        let session_data = get_full_session_by_id(session_id)
            .map_err(|e| format!("Failed to get session data: {}", e))?
            .ok_or_else(|| format!("Session {} not found in database", session_id))?;

        // Extract and upload project metadata if CWD is available
        let final_project_name = if let Some(ref cwd) = item.cwd {
            use crate::project_metadata::extract_project_metadata;

            log_info(
                "upload-queue",
                &format!("📁 Extracting project metadata from CWD: {}", cwd),
            )
            .unwrap_or_default();

            match extract_project_metadata(cwd) {
                Ok(metadata) => {
                    log_info(
                        "upload-queue",
                        &format!(
                            "✓ Extracted project: {} (type: {}, git: {})",
                            metadata.project_name,
                            metadata.detected_project_type,
                            metadata.git_remote_url.as_deref().unwrap_or("none")
                        ),
                    )
                    .unwrap_or_default();

                    // Upload project metadata to server
                    if let Err(e) = Self::upload_project_metadata_static(&metadata, Some(config.clone())).await {
                        log_warn("upload-queue", &format!("⚠ Failed to upload project metadata: {}", e))
                            .unwrap_or_default();
                    }

                    metadata.project_name
                }
                Err(e) => {
                    log_warn(
                        "upload-queue",
                        &format!("⚠ Could not extract project metadata: {} - using folder name", e),
                    )
                    .unwrap_or_default();
                    item.project_name.clone()
                }
            }
        } else {
            item.project_name.clone()
        };

        // Helper to convert timestamp to ISO string
        let timestamp_to_iso = |ts_ms: Option<i64>| -> Option<String> {
            ts_ms.map(|ms| {
                DateTime::from_timestamp_millis(ms)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            })
        };

        // Get rating if available
        let rating = get_session_rating(session_id).ok().flatten();

        // Prepare session upload request without content
        let session_request = serde_json::json!({
            "provider": session_data.provider,
            "projectName": final_project_name,
            "sessionId": session_data.session_id,
            "fileName": session_data.file_name,
            "filePath": session_data.file_path,
            // No content field - metrics only
            "sessionStartTime": timestamp_to_iso(session_data.session_start_time),
            "sessionEndTime": timestamp_to_iso(session_data.session_end_time),
            "durationMs": session_data.duration_ms,
            "processingStatus": session_data.processing_status,
            "queuedAt": timestamp_to_iso(session_data.queued_at),
            "processedAt": timestamp_to_iso(session_data.processed_at),
            "coreMetricsStatus": session_data.core_metrics_status,
            "coreMetricsProcessedAt": timestamp_to_iso(session_data.core_metrics_processed_at),
            "assessmentStatus": session_data.assessment_status,
            "assessmentCompletedAt": timestamp_to_iso(session_data.assessment_completed_at),
            "assessmentRating": rating,
            "aiModelSummary": session_data.ai_model_summary,
            "aiModelQualityScore": session_data.ai_model_quality_score,
            "aiModelMetadata": session_data.ai_model_metadata.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
            "aiModelPhaseAnalysis": session_data.ai_model_phase_analysis.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        });

        // Upload session metadata
        let client = reqwest::Client::new();
        let url = format!("{}/api/agent-sessions/upload", server_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&session_request)
            .send()
            .await
            .map_err(|e| format!("Failed to upload session metadata: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Session metadata upload failed (metrics-only mode) with status {}: {}", status, error_text));
        }

        log_info(
            "upload-queue",
            &format!("✓ Uploaded session metadata for {}", session_id),
        )
        .unwrap_or_default();

        // Fetch and upload metrics
        if let Ok(Some(metrics)) = get_session_metrics(session_id) {
            Self::upload_session_metrics(&metrics, &server_url, &api_key).await?;
        } else {
            log_warn(
                "upload-queue",
                &format!("⚠ No metrics found for session {}", session_id),
            )
            .unwrap_or_default();
        }

        Ok(())
    }

    /// Upload session transcript (JSONL) and metrics
    async fn upload_transcript_and_metrics(
        item: &UploadItem,
        config: GuideAIConfig,
    ) -> Result<(), String> {
        let api_key = config.api_key.clone().ok_or("No API key configured")?;
        let server_url = config
            .server_url
            .clone()
            .ok_or("No server URL configured")?;

        // Extract and upload project metadata if CWD is available
        let final_project_name = if let Some(ref cwd) = item.cwd {
            use crate::project_metadata::extract_project_metadata;

            log_info(
                "upload-queue",
                &format!("📁 Extracting project metadata from CWD: {}", cwd),
            )
            .unwrap_or_default();

            match extract_project_metadata(cwd) {
                Ok(metadata) => {
                    log_info(
                        "upload-queue",
                        &format!(
                            "✓ Extracted project: {} (type: {}, git: {})",
                            metadata.project_name,
                            metadata.detected_project_type,
                            metadata.git_remote_url.as_deref().unwrap_or("none")
                        ),
                    )
                    .unwrap_or_default();

                    // Upload project metadata to server
                    if let Err(e) = Self::upload_project_metadata_static(&metadata, Some(config.clone())).await {
                        log_warn("upload-queue", &format!("⚠ Failed to upload project metadata: {} - continuing with session upload", e))
                            .unwrap_or_default();
                    } else {
                        log_info(
                            "upload-queue",
                            &format!("✓ Project metadata uploaded: {}", metadata.project_name),
                        )
                        .unwrap_or_default();
                    }

                    // Use real project name from metadata instead of folder name
                    metadata.project_name
                }
                Err(e) => {
                    log_warn(
                        "upload-queue",
                        &format!(
                            "⚠ Could not extract project metadata from {}: {} - using folder name",
                            cwd, e
                        ),
                    )
                    .unwrap_or_default();
                    item.project_name.clone()
                }
            }
        } else {
            log_debug(
                "upload-queue",
                "No CWD available for project metadata extraction",
            )
            .unwrap_or_default();
            item.project_name.clone()
        };

        // Get content - all providers now use file paths (OpenCode uses cached JSONL)
        let encoded_content = if let Some(ref content) = item.content {
            // Use in-memory content if available (legacy path, rarely used)
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(content.as_bytes())
        } else {
            // All providers: Read file content directly from cached JSONL
            // OpenCode sessions are now cached at ~/.guideai/cache/opencode/{session_id}.jsonl
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

        // Get full session data from database to include all fields
        use crate::database::{get_full_session_by_id, get_session_rating};

        let session_data = get_full_session_by_id(&session_id)
            .map_err(|e| format!("Failed to get session data: {}", e))?
            .ok_or_else(|| format!("Session {} not found in database", session_id))?;

        // Helper to convert timestamp to ISO string
        let timestamp_to_iso = |ts_ms: Option<i64>| -> Option<String> {
            ts_ms.map(|ms| {
                DateTime::from_timestamp_millis(ms)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            })
        };

        // Get rating if available
        let rating = get_session_rating(&session_id).ok().flatten();

        // Prepare upload request with ALL session fields (use final_project_name which may be the real project name from metadata)
        let upload_request = serde_json::json!({
            "provider": item.provider.clone(),
            "projectName": final_project_name,
            "sessionId": session_id.clone(),
            "fileName": item.file_name.clone(),
            "filePath": item.file_path.to_string_lossy().to_string(),
            "content": encoded_content,
            // Include all session metadata fields
            "sessionStartTime": timestamp_to_iso(session_data.session_start_time),
            "sessionEndTime": timestamp_to_iso(session_data.session_end_time),
            "durationMs": session_data.duration_ms,
            "processingStatus": session_data.processing_status,
            "queuedAt": timestamp_to_iso(session_data.queued_at),
            "processedAt": timestamp_to_iso(session_data.processed_at),
            "coreMetricsStatus": session_data.core_metrics_status,
            "coreMetricsProcessedAt": timestamp_to_iso(session_data.core_metrics_processed_at),
            "assessmentStatus": session_data.assessment_status,
            "assessmentCompletedAt": timestamp_to_iso(session_data.assessment_completed_at),
            "assessmentRating": rating,
            "aiModelSummary": session_data.ai_model_summary,
            "aiModelQualityScore": session_data.ai_model_quality_score,
            "aiModelMetadata": session_data.ai_model_metadata.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
            "aiModelPhaseAnalysis": session_data.ai_model_phase_analysis.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        });

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

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!(
                "Session transcript upload failed (full sync mode) with status {}: {}",
                status, error_text
            ));
        }

        log_info(
            "upload-queue",
            &format!("✓ Uploaded session transcript for {}", session_id),
        )
        .unwrap_or_default();

        // Also upload metrics if available
        use crate::database::get_session_metrics;
        if let Ok(Some(metrics)) = get_session_metrics(&session_id) {
            // Propagate metrics upload error to prevent marking session as synced
            Self::upload_session_metrics(&metrics, &server_url, &api_key).await?;
        }

        Ok(())
    }

    /// Helper function to upload session metrics to server
    async fn upload_session_metrics(
        metrics: &crate::database::SessionMetrics,
        server_url: &str,
        api_key: &str,
    ) -> Result<(), String> {
        // Helper to parse JSON array from comma-separated string
        let parse_array = |s: &Option<String>| -> Option<Vec<String>> {
            s.as_ref().map(|str_val| {
                str_val
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim().to_string())
                    .collect()
            })
        };

        // Prepare metrics upload request
        let metrics_request = serde_json::json!({
            "metrics": [{
                "sessionId": metrics.session_id,
                "provider": metrics.provider,
                // Performance metrics
                "responseLatencyMs": metrics.response_latency_ms,
                "taskCompletionTimeMs": metrics.task_completion_time_ms,
                "performanceTotalResponses": metrics.performance_total_responses,
                // Usage metrics
                "readWriteRatio": metrics.read_write_ratio,
                "inputClarityScore": metrics.input_clarity_score,
                "readOperations": metrics.read_operations,
                "writeOperations": metrics.write_operations,
                "totalUserMessages": metrics.total_user_messages,
                // Error metrics
                "errorCount": metrics.error_count,
                "errorTypes": parse_array(&metrics.error_types),
                "lastErrorMessage": metrics.last_error_message,
                "recoveryAttempts": metrics.recovery_attempts,
                "fatalErrors": metrics.fatal_errors,
                // Engagement metrics
                "interruptionRate": metrics.interruption_rate,
                "sessionLengthMinutes": metrics.session_length_minutes,
                "totalInterruptions": metrics.total_interruptions,
                "engagementTotalResponses": metrics.engagement_total_responses,
                // Quality metrics
                "taskSuccessRate": metrics.task_success_rate,
                "iterationCount": metrics.iteration_count,
                "processQualityScore": metrics.process_quality_score,
                "usedPlanMode": metrics.used_plan_mode,
                "usedTodoTracking": metrics.used_todo_tracking,
                "overTopAffirmations": metrics.over_top_affirmations,
                "successfulOperations": metrics.successful_operations,
                "totalOperations": metrics.total_operations,
                "exitPlanModeCount": metrics.exit_plan_mode_count,
                "todoWriteCount": metrics.todo_write_count,
                "overTopAffirmationsPhrases": parse_array(&metrics.over_top_affirmations_phrases),
                "improvementTips": parse_array(&metrics.improvement_tips),
                // Custom metrics
                "customMetrics": metrics.custom_metrics.as_ref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
            }]
        });

        // Upload metrics
        let client = reqwest::Client::new();
        let url = format!("{}/api/session-metrics/upload", server_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&metrics_request)
            .send()
            .await
            .map_err(|e| format!("Failed to upload metrics: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Session metrics upload failed with status {}: {}", status, error_text));
        }

        log_info(
            "upload-queue",
            &format!("✓ Uploaded metrics for session {}", metrics.session_id),
        )
        .unwrap_or_default();

        Ok(())
    }

    pub fn clear_failed(&self) {
        // Clear failed sessions from database by deleting them
        use crate::database::clear_failed_sessions;
        let _ = clear_failed_sessions();
    }

    pub fn retry_failed(&self) {
        // Retry failed sessions by clearing sync_failed_reason and resetting synced_to_server
        use crate::database::retry_failed_sessions;
        let _ = retry_failed_sessions();
    }

    pub fn clear_uploaded_hashes(&self) {
        if let Ok(mut uploaded_hashes) = self.uploaded_hashes.lock() {
            uploaded_hashes.clear();
        }
    }

    pub fn get_all_items(&self) -> QueueItems {
        use crate::database::{get_failed_sessions, get_unsynced_sessions};

        // Get pending items from database (unsynced sessions)
        let pending = if let Ok(unsynced_sessions) = get_unsynced_sessions() {
            unsynced_sessions
                .into_iter()
                .map(|session| {
                    UploadItem {
                        id: session.id,
                        provider: session.provider,
                        project_name: session.project_name,
                        file_path: PathBuf::from(&session.file_path),
                        file_name: session.file_name,
                        queued_at: Utc::now(), // Use current time as approximation
                        retry_count: 0,
                        next_retry_at: None,
                        last_error: None,
                        file_hash: None,
                        file_size: session.file_size as u64,
                        session_id: Some(session.session_id),
                        content: None,
                        cwd: session.cwd,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        // Get failed items from database (sessions with sync_failed_reason)
        let failed = if let Ok(failed_sessions) = get_failed_sessions() {
            failed_sessions
                .into_iter()
                .map(|session| {
                    UploadItem {
                        id: session.id,
                        provider: session.provider,
                        project_name: session.project_name,
                        file_path: PathBuf::from(&session.file_path),
                        file_name: session.file_name,
                        queued_at: Utc::now(), // Use current time as approximation
                        retry_count: 3,        // Max retries exceeded
                        next_retry_at: None,
                        last_error: Some(session.sync_failed_reason),
                        file_hash: None,
                        file_size: session.file_size as u64,
                        session_id: Some(session.session_id),
                        content: None,
                        cwd: session.cwd,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        QueueItems { pending, failed }
    }

    pub fn remove_item(&self, item_id: &str) -> Result<(), String> {
        // Remove session from database by ID
        use crate::database::remove_session_by_id;

        let rows_affected = remove_session_by_id(item_id)
            .map_err(|e| format!("Failed to remove item: {}", e))?;

        if rows_affected > 0 {
            Ok(())
        } else {
            Err("Item not found in database".to_string())
        }
    }

    pub fn retry_item(&self, item_id: &str) -> Result<(), String> {
        // Retry failed session by clearing sync_failed_reason and resetting synced_to_server
        use crate::database::retry_session_by_id;

        let rows_affected = retry_session_by_id(item_id)
            .map_err(|e| format!("Failed to retry item: {}", e))?;

        if rows_affected > 0 {
            Ok(())
        } else {
            Err("Item not found or not in failed state".to_string())
        }
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

    /// Upload project metadata to the server (static version for use in async tasks)
    async fn upload_project_metadata_static(
        metadata: &ProjectMetadata,
        config: Option<GuideAIConfig>,
    ) -> Result<(), String> {
        let config = config.ok_or("No configuration available")?;
        let api_key = config.api_key.ok_or("No API key configured")?;
        let server_url = config.server_url.ok_or("No server URL configured")?;

        // Prepare upload request
        let upload_request = ProjectUploadRequest {
            project_name: metadata.project_name.clone(),
            git_remote_url: metadata.git_remote_url.clone(),
            cwd: metadata.cwd.clone(),
            detected_project_type: metadata.detected_project_type.clone(),
        };

        // Make HTTP POST request to server
        let client = reqwest::Client::new();
        let url = format!("{}/api/projects", server_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&upload_request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if response.status().is_success() {
            log_info(
                "upload-queue",
                &format!("📦 Project metadata uploaded: {}", metadata.project_name),
            )
            .unwrap_or_default();
            Ok(())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(format!(
                "Project upload failed with status {}: {}",
                status, error_text
            ))
        }
    }

    /// Upload project metadata to the server
    #[allow(dead_code)]
    pub async fn upload_project_metadata(&self, metadata: &ProjectMetadata) -> Result<(), String> {
        let config = if let Ok(config_guard) = self.config.lock() {
            config_guard.clone()
        } else {
            None
        };

        Self::upload_project_metadata_static(metadata, config).await
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
        use std::fs;

        let queue = UploadQueue::new();

        // Create temp file in allowed directory (~/.guideai/test)
        let home = dirs::home_dir().unwrap();
        let test_dir = home.join(".guideai").join("test");
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test_session.jsonl");
        let jsonl_content =
            r#"{"timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":"test"}"#;
        fs::write(&test_file, jsonl_content).unwrap();

        let result = queue.add_item(
            "claude-code",
            "test-project",
            test_file.clone(),
        );

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
        let (is_valid, error) = UploadQueue::validate_jsonl_timestamps(content);
        assert!(is_valid, "Validation failed: {:?}", error);

        let result = queue.add_session_content(
            "opencode",
            "test-project",
            "test-session",
            content.to_string(),
        );
        assert!(result.is_ok(), "add_session_content failed: {:?}", result.err());

        // Check in-memory queue directly since get_status() reads from database
        let queue_len = queue.queue.lock().unwrap().len();
        assert_eq!(queue_len, 1, "Expected 1 item in queue, got {}", queue_len);
    }

    #[test]
    fn test_validate_jsonl_timestamps() {
        // Valid JSONL with timestamp
        let valid_content =
            r#"{"timestamp":"2025-01-01T10:00:00.000Z","type":"user","message":"test"}"#;
        let (is_valid, error) = UploadQueue::validate_jsonl_timestamps(valid_content);
        assert!(is_valid);
        assert!(error.is_none());

        // Invalid JSONL without timestamp
        let invalid_content = r#"{"type":"user","message":"test"}"#;
        let (is_valid, error) = UploadQueue::validate_jsonl_timestamps(invalid_content);
        assert!(!is_valid);
        assert!(error.is_some());

        // Empty content
        let (is_valid, error) = UploadQueue::validate_jsonl_timestamps("");
        assert!(!is_valid);
        assert!(error.is_some());

        // Multiple lines with at least one timestamp
        let mixed_content = r#"{"type":"user","message":"test"}
{"timestamp":"2025-01-01T10:00:00.000Z","type":"assistant","message":"response"}"#;
        let (is_valid, error) = UploadQueue::validate_jsonl_timestamps(mixed_content);
        assert!(is_valid);
        assert!(error.is_none());
    }

    #[test]
    fn test_add_item_without_timestamps() {
        use std::fs;

        let queue = UploadQueue::new();

        // Create temp file in allowed directory (~/.guideai/test)
        let home = dirs::home_dir().unwrap();
        let test_dir = home.join(".guideai").join("test");
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test_session_no_timestamps.jsonl");
        let jsonl_content = r#"{"type":"user","message":"test"}"#;
        fs::write(&test_file, jsonl_content).unwrap();

        let result = queue.add_item(
            "claude-code",
            "test-project",
            test_file.clone(),
        );

        // Clean up
        fs::remove_file(&test_file).ok();

        assert!(result.is_ok(), "add_item failed: {:?}", result.err());

        // Should not be added to queue due to missing timestamps
        let status = queue.get_status();
        assert_eq!(status.pending, 0);
    }
}
