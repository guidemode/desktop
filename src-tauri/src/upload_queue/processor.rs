//! Main upload processor with concurrent task management.
//!
//! Handles the processing loop, database polling, and upload orchestration.
//! Refactored from 297-line monolithic function into focused methods.

use crate::config::GuideAIConfig;
use crate::database::{get_unsynced_sessions, mark_session_sync_failed, mark_session_synced};
use crate::logging::{log_error, log_info, log_warn};
use chrono::{DateTime, Utc};
use indexmap::IndexSet;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::Semaphore;
use tokio::time::sleep;

use super::queue_manager;
use super::types::{UploadItem, DB_POLL_INTERVAL_SECS, MAX_UPLOADED_HASHES};
use super::upload::{process_upload_item, classify_error, should_retry, schedule_retry, calculate_backoff, ErrorType};

/// Main upload processor that manages the processing loop
#[derive(Clone)]
pub struct UploadProcessor {
    queue: Arc<Mutex<VecDeque<UploadItem>>>,
    processing: Arc<Mutex<usize>>,
    failed_items: Arc<Mutex<Vec<UploadItem>>>,
    uploaded_hashes: Arc<Mutex<IndexSet<String>>>,
    is_running: Arc<Mutex<bool>>,
    config: Arc<Mutex<Option<GuideAIConfig>>>,
    app_handle: Arc<Mutex<Option<tauri::AppHandle>>>,
    semaphore: Arc<Semaphore>,
}

impl UploadProcessor {
    /// Create a new processor with all required state
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        queue: Arc<Mutex<VecDeque<UploadItem>>>,
        processing: Arc<Mutex<usize>>,
        failed_items: Arc<Mutex<Vec<UploadItem>>>,
        uploaded_hashes: Arc<Mutex<IndexSet<String>>>,
        is_running: Arc<Mutex<bool>>,
        config: Arc<Mutex<Option<GuideAIConfig>>>,
        app_handle: Arc<Mutex<Option<tauri::AppHandle>>>,
        semaphore: Arc<Semaphore>,
    ) -> Self {
        Self {
            queue,
            processing,
            failed_items,
            uploaded_hashes,
            is_running,
            config,
            app_handle,
            semaphore,
        }
    }

    /// Start the processing loop (main entry point)
    pub fn start(&self) -> Result<(), String> {
        // Check if already running
        if !self.try_start()? {
            return Ok(());
        }

        let processor = self.clone();

        tauri::async_runtime::spawn(async move {
            log_info("upload-queue", "📤 Upload processor started").unwrap_or_default();
            processor.run_loop().await;
            log_info("upload-queue", "📤 Upload processor stopped").unwrap_or_default();
        });

        Ok(())
    }

    /// Main processing loop
    async fn run_loop(&self) {
        let mut last_db_poll = Utc::now();

        loop {
            // Check if we should continue
            if !self.should_continue() {
                break;
            }

            // Check authentication
            if !self.has_valid_auth() {
                sleep(Duration::from_millis(500)).await;
                continue;
            }

            // Poll database if needed
            if let Err(e) = self.poll_database_if_needed(&mut last_db_poll).await {
                log_error(
                    "upload-queue",
                    &format!("Database polling failed: {}", e),
                )
                .unwrap_or_default();
            }

            // Process available items
            self.process_available_items().await;

            // Brief sleep to avoid busy-waiting
            sleep(Duration::from_millis(500)).await;
        }
    }

    /// Check if processor should continue running
    fn should_continue(&self) -> bool {
        self.is_running
            .lock()
            .map(|running| *running)
            .unwrap_or(false)
    }

    /// Check if we have valid authentication configured
    fn has_valid_auth(&self) -> bool {
        self.config
            .lock()
            .ok()
            .and_then(|config| {
                config.as_ref().map(|cfg| {
                    cfg.api_key.is_some() && cfg.server_url.is_some() && cfg.tenant_id.is_some()
                })
            })
            .unwrap_or(false)
    }

    /// Poll database for unsynced sessions if interval has elapsed
    async fn poll_database_if_needed(
        &self,
        last_db_poll: &mut DateTime<Utc>,
    ) -> Result<(), String> {
        let now = Utc::now();
        let elapsed = (now - *last_db_poll).num_seconds();

        if elapsed < DB_POLL_INTERVAL_SECS as i64 {
            return Ok(());
        }

        *last_db_poll = now;
        self.fetch_and_queue_unsynced_sessions().await
    }

    /// Fetch unsynced sessions from database and add to queue
    async fn fetch_and_queue_unsynced_sessions(&self) -> Result<(), String> {
        let unsynced = get_unsynced_sessions()
            .map_err(|e| format!("Failed to get unsynced sessions: {}", e))?;

        if unsynced.is_empty() {
            return Ok(());
        }

        log_info(
            "upload-queue",
            &format!(
                "📊 Found {} unsynced sessions in database",
                unsynced.len()
            ),
        )
        .unwrap_or_default();

        let mut queue = self.queue.lock().unwrap();

        for session in unsynced {
            if self.is_session_queued(&queue, &session.session_id) {
                continue;
            }

            let item = UploadItem {
                id: session.id.clone(),
                provider: session.provider.clone(),
                project_name: session.project_name.clone(),
                file_path: PathBuf::from(&session.file_path),
                file_name: session.file_name.clone(),
                queued_at: Utc::now(),
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

        Ok(())
    }

    /// Check if session is already in queue
    fn is_session_queued(&self, queue: &VecDeque<UploadItem>, session_id: &str) -> bool {
        queue
            .iter()
            .any(|item| item.session_id.as_ref() == Some(&session_id.to_string()))
    }

    /// Process available items up to semaphore limit
    async fn process_available_items(&self) {
        let available_permits = self.semaphore.available_permits();

        for _ in 0..available_permits {
            // Try to get an item
            let item = self.get_next_item();

            if item.is_none() {
                break; // No more items
            }

            // Spawn upload task
            self.spawn_upload_task(item.unwrap());
        }
    }

    /// Get next item from queue (prioritizing ready retries)
    fn get_next_item(&self) -> Option<UploadItem> {
        let mut queue = self.queue.lock().unwrap();

        // First, check for ready retry items
        if let Some(item) = queue_manager::find_ready_item(&mut queue) {
            return Some(item);
        }

        // Otherwise, get first item
        queue.pop_front()
    }

    /// Spawn async task to upload an item
    fn spawn_upload_task(&self, item: UploadItem) {
        let semaphore = Arc::clone(&self.semaphore);
        let processing = Arc::clone(&self.processing);
        let config = Arc::clone(&self.config);
        let app_handle = Arc::clone(&self.app_handle);
        let queue = Arc::clone(&self.queue);
        let failed_items = Arc::clone(&self.failed_items);
        let uploaded_hashes = Arc::clone(&self.uploaded_hashes);

        tauri::async_runtime::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            // Increment processing counter
            increment_counter(&processing);

            // Get config
            let upload_config = get_config(&config);

            // Log upload attempt
            log_info(
                "upload-queue",
                &format!(
                    "📤 Uploading {} to server (attempt {})",
                    item.file_name,
                    item.retry_count + 1
                ),
            )
            .unwrap_or_default();

            // Process upload
            let item_mut = item.clone();
            let result = process_upload_item(&item_mut, upload_config).await;

            // Handle result
            match result {
                Ok(_) => {
                    handle_upload_success(item_mut, &uploaded_hashes, &app_handle).await;
                }
                Err(e) => {
                    handle_upload_failure(
                        item_mut,
                        e,
                        &queue,
                        &failed_items,
                        &app_handle,
                    )
                    .await;
                }
            }

            // Decrement processing counter
            decrement_counter(&processing);
        });
    }

    /// Try to mark processor as running
    fn try_start(&self) -> Result<bool, String> {
        let mut is_running = self
            .is_running
            .lock()
            .map_err(|_| "Failed to lock is_running")?;

        if *is_running {
            return Ok(false);
        }

        *is_running = true;
        Ok(true)
    }
}

// Helper functions

fn increment_counter(counter: &Arc<Mutex<usize>>) {
    if let Ok(mut count) = counter.lock() {
        *count += 1;
    }
}

fn decrement_counter(counter: &Arc<Mutex<usize>>) {
    if let Ok(mut count) = counter.lock() {
        *count = count.saturating_sub(1);
    }
}

fn get_config(config: &Arc<Mutex<Option<GuideAIConfig>>>) -> Option<GuideAIConfig> {
    config.lock().ok().and_then(|c| c.clone())
}

async fn handle_upload_success(
    item: UploadItem,
    uploaded_hashes: &Arc<Mutex<IndexSet<String>>>,
    app_handle: &Arc<Mutex<Option<tauri::AppHandle>>>,
) {
    // Mark hash as uploaded
    if let Some(file_hash) = &item.file_hash {
        if let Ok(mut hashes) = uploaded_hashes.lock() {
            hashes.insert(file_hash.clone());

            // Prune oldest entries if cache exceeds the limit
            if hashes.len() > MAX_UPLOADED_HASHES {
                // Keep the most recent 100 entries (1% of max)
                let keep_count = MAX_UPLOADED_HASHES / 100;
                let remove_count = hashes.len() - keep_count;

                log_info(
                    "upload-queue",
                    &format!(
                        "Uploaded hashes cache exceeded limit ({}), pruning {} oldest entries, keeping {}",
                        MAX_UPLOADED_HASHES, remove_count, keep_count
                    ),
                )
                .unwrap_or_default();

                // Remove oldest entries (shift_remove removes from the front)
                for _ in 0..remove_count {
                    hashes.shift_remove_index(0);
                }
            }
        }
    }

    // Mark session as synced in database
    if let Some(ref session_id) = item.session_id {
        if let Err(e) = mark_session_synced(session_id, None) {
            log_error(
                "upload-queue",
                &format!("Failed to mark session {} as synced: {}", session_id, e),
            )
            .unwrap_or_default();
        } else {
            emit_session_event(app_handle, "session-synced", session_id).await;
        }
    }

    log_info(
        "upload-queue",
        &format!(
            "✓ Upload successful: {} (size: {} bytes)",
            item.file_name, item.file_size
        ),
    )
    .unwrap_or_default();
}

async fn handle_upload_failure(
    mut item: UploadItem,
    error: String,
    queue: &Arc<Mutex<VecDeque<UploadItem>>>,
    failed_items: &Arc<Mutex<Vec<UploadItem>>>,
    app_handle: &Arc<Mutex<Option<tauri::AppHandle>>>,
) {
    item.last_error = Some(error.clone());

    // Use retry module to classify error
    let error_type = classify_error(&error);

    match error_type {
        ErrorType::Client => {
            // Don't retry client errors (400, invalid input)
            move_to_failed(&item, failed_items);
            mark_session_as_failed(&item, &error, app_handle).await;

            log_error(
                "upload-queue",
                &format!(
                    "✗ Upload failed (invalid input, will not retry): {} - Error: {}",
                    item.file_name, error
                ),
            )
            .unwrap_or_default();
        }
        ErrorType::Server | ErrorType::Network => {
            // Retry with backoff
            item.retry_count += 1;

            // Use retry module to check if we should retry
            if should_retry(&item, error_type) {
                // Use retry module to schedule retry
                schedule_retry(&mut item);
                requeue_item(item.clone(), queue);

                // Use retry module to calculate backoff
                let delay_seconds = calculate_backoff(item.retry_count - 1);
                log_warn(
                    "upload-queue",
                    &format!(
                        "⚠ Upload failed, retrying {} in {}s: {}",
                        item.file_name, delay_seconds, error
                    ),
                )
                .unwrap_or_default();
            } else {
                move_to_failed(&item, failed_items);
                mark_session_as_failed(&item, &error, app_handle).await;

                log_error(
                    "upload-queue",
                    &format!(
                        "✗ Upload failed permanently: {} (after {} attempts)",
                        item.file_name, item.retry_count
                    ),
                )
                .unwrap_or_default();
            }
        }
    }
}

fn requeue_item(item: UploadItem, queue: &Arc<Mutex<VecDeque<UploadItem>>>) {
    if let Ok(mut q) = queue.lock() {
        q.push_back(item);
    }
}

fn move_to_failed(item: &UploadItem, failed_items: &Arc<Mutex<Vec<UploadItem>>>) {
    if let Ok(mut failed) = failed_items.lock() {
        failed.push(item.clone());
    }
}

async fn mark_session_as_failed(
    item: &UploadItem,
    error: &str,
    app_handle: &Arc<Mutex<Option<tauri::AppHandle>>>,
) {
    if let Some(ref session_id) = item.session_id {
        if let Err(e) = mark_session_sync_failed(session_id, error) {
            log_error(
                "upload-queue",
                &format!(
                    "Failed to mark session {} as sync failed: {}",
                    session_id, e
                ),
            )
            .unwrap_or_default();
        } else {
            emit_session_event(app_handle, "session-sync-failed", session_id).await;
        }
    }
}

async fn emit_session_event(
    app_handle: &Arc<Mutex<Option<tauri::AppHandle>>>,
    event: &str,
    session_id: &str,
) {
    if let Ok(handle_guard) = app_handle.lock() {
        if let Some(ref handle) = *handle_guard {
            let _ = handle.emit(event, session_id);
        }
    }
}
