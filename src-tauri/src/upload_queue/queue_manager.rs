//! Queue operations - add, remove, retry, and query upload items.
//!
//! Manages queue state and database integration for upload items.

use crate::config::GuideAIConfig;
use crate::database::{
    clear_failed_sessions, get_failed_sessions, get_unsynced_sessions, get_upload_stats,
    remove_session_by_id, retry_failed_sessions, retry_session_by_id,
};
use crate::logging::{log_info, log_warn};
use crate::project_metadata::extract_project_metadata;
use crate::providers::SessionInfo;
use crate::validation::{validate_session_file, MAX_SESSION_FILE_SIZE};
use chrono::Utc;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::hashing::{calculate_content_hash_sha256, calculate_file_hash_sha256};
use super::types::{QueueItems, UploadItem, UploadStatus};
use super::validation::validate_jsonl_timestamps;

/// Add a file-based upload item to the queue (test only)
#[cfg(test)]
pub fn add_item(
    queue: &Arc<Mutex<VecDeque<UploadItem>>>,
    uploaded_hashes: &Arc<Mutex<HashSet<String>>>,
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

    let (is_valid, validation_error) = validate_jsonl_timestamps(&file_content);
    if !is_valid {
        let reason = validation_error.unwrap_or_else(|| "no valid timestamps found".to_string());
        log_warn(
            "upload-queue",
            &format!("⚠ Skipping upload: {} ({})", file_name, reason),
        )
        .unwrap_or_default();
        return Ok(());
    }

    // Calculate file hash for deduplication (SHA256)
    let file_hash = calculate_file_hash_sha256(&validated_path)?;

    // Check if this file has already been uploaded
    if is_file_already_uploaded(uploaded_hashes, &file_hash) {
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

    if let Ok(mut queue) = queue.lock() {
        queue.push_back(item.clone());
    }

    Ok(())
}

/// Add a historical session to the queue
pub fn add_historical_session(
    queue: &Arc<Mutex<VecDeque<UploadItem>>>,
    uploaded_hashes: &Arc<Mutex<HashSet<String>>>,
    _config: &Arc<Mutex<Option<GuideAIConfig>>>,
    session: &SessionInfo,
) -> Result<(), String> {
    // Handle sessions with in-memory content vs file-based sessions differently
    let (file_hash, file_size, content): (String, u64, Option<String>) =
        if let Some(ref content) = session.content {
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

            let (is_valid, validation_error) = validate_jsonl_timestamps(content);
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

            let content_hash = calculate_content_hash_sha256(content);
            (content_hash, content_size, Some(content.clone()))
        } else {
            // For file-based sessions, validate path and check file size
            let (validated_path, file_size) =
                validate_session_file(&session.file_path).map_err(|e| e.to_string())?;

            // Read and validate content
            let file_content = std::fs::read_to_string(&validated_path)
                .map_err(|e| format!("Failed to read file: {}", e))?;

            let (is_valid, validation_error) = validate_jsonl_timestamps(&file_content);
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

            let file_hash = calculate_file_hash_sha256(&validated_path)?;
            (file_hash, file_size, None)
        };

    // Check if this file has already been uploaded
    if is_file_already_uploaded(uploaded_hashes, &file_hash) {
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

    // Extract project metadata if CWD is available (will be embedded in upload payload)
    let real_project_name = if let Some(ref cwd) = session.cwd {
        log_info("upload-queue", &format!("📁 Extracting project metadata from CWD: {} (for session {}, Claude folder: {})", cwd, session.session_id, session.project_name))
            .unwrap_or_default();

        match extract_project_metadata(cwd) {
            Ok(metadata) => {
                log_info("upload-queue", &format!("✓ Extracted project metadata: {} (type: {}, git: {}) - will embed in upload payload",
                    metadata.project_name,
                    metadata.detected_project_type,
                    metadata.git_remote_url.as_deref().unwrap_or("none")
                )).unwrap_or_default();

                // Project metadata will be embedded in the upload payload (v2/metrics)
                // No separate /api/projects call needed anymore

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
    let project_name_for_upload = real_project_name.unwrap_or_else(|| session.project_name.clone());

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

    if let Ok(mut queue) = queue.lock() {
        queue.push_back(item.clone());
    }

    Ok(())
}

/// Add session content (in-memory) to the queue (test only)
#[cfg(test)]
pub fn add_session_content(
    queue: &Arc<Mutex<VecDeque<UploadItem>>>,
    uploaded_hashes: &Arc<Mutex<HashSet<String>>>,
    provider: &str,
    project_name: &str,
    session_id: &str,
    content: String,
) -> Result<(), String> {
    // Validate content before adding to queue
    let (is_valid, validation_error) = validate_jsonl_timestamps(&content);
    if !is_valid {
        let reason = validation_error.unwrap_or_else(|| "no valid timestamps found".to_string());
        log_warn(
            "upload-queue",
            &format!("⚠ Skipping session upload: {} ({})", session_id, reason),
        )
        .unwrap_or_default();
        return Ok(());
    }

    // Calculate content hash for deduplication (SHA256)
    let content_hash = calculate_content_hash_sha256(&content);

    let content_size = content.len() as u64;

    // Check if this content has already been uploaded
    if is_file_already_uploaded(uploaded_hashes, &content_hash) {
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

    if let Ok(mut queue) = queue.lock() {
        queue.push_back(item.clone());
    }

    Ok(())
}

/// Get upload queue status
pub fn get_status(processing: &Arc<Mutex<usize>>) -> UploadStatus {
    // Get real-time stats from database instead of in-memory queue
    let db_stats = get_upload_stats().unwrap_or(crate::database::UploadStats {
        pending: 0,
        synced: 0,
        total: 0,
    });

    let processing_count = if let Ok(processing) = processing.lock() {
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
        processing: processing_count,
        failed,
        recent_uploads,
    }
}

/// Get all queue items (pending and failed)
pub fn get_all_items() -> QueueItems {
    // Get pending items from database (unsynced sessions)
    let pending = if let Ok(unsynced_sessions) = get_unsynced_sessions() {
        unsynced_sessions
            .into_iter()
            .map(|session| UploadItem {
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
            })
            .collect()
    } else {
        Vec::new()
    };

    // Get failed items from database (sessions with sync_failed_reason)
    let failed = if let Ok(failed_sessions) = get_failed_sessions() {
        failed_sessions
            .into_iter()
            .map(|session| UploadItem {
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
            })
            .collect()
    } else {
        Vec::new()
    };

    QueueItems { pending, failed }
}

/// Remove an item from the queue by ID
pub fn remove_item(item_id: &str) -> Result<(), String> {
    // Remove session from database by ID
    let rows_affected =
        remove_session_by_id(item_id).map_err(|e| format!("Failed to remove item: {}", e))?;

    if rows_affected > 0 {
        Ok(())
    } else {
        Err("Item not found in database".to_string())
    }
}

/// Retry a failed item by ID
pub fn retry_item(item_id: &str) -> Result<(), String> {
    // Retry failed session by clearing sync_failed_reason and resetting synced_to_server
    let rows_affected =
        retry_session_by_id(item_id).map_err(|e| format!("Failed to retry item: {}", e))?;

    if rows_affected > 0 {
        Ok(())
    } else {
        Err("Item not found or not in failed state".to_string())
    }
}

/// Clear all failed items
pub fn clear_failed() {
    // Clear failed sessions from database by deleting them
    let _ = clear_failed_sessions();
}

/// Retry all failed items
pub fn retry_failed() {
    // Retry failed sessions by clearing sync_failed_reason and resetting synced_to_server
    let _ = retry_failed_sessions();
}

/// Find an item in the queue that's ready to retry
pub fn find_ready_item(queue: &mut VecDeque<UploadItem>) -> Option<UploadItem> {
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

/// Check if a file hash has already been uploaded
pub fn is_file_already_uploaded(
    uploaded_hashes: &Arc<Mutex<HashSet<String>>>,
    file_hash: &str,
) -> bool {
    if let Ok(uploaded_hashes) = uploaded_hashes.lock() {
        uploaded_hashes.contains(file_hash)
    } else {
        false
    }
}

// DEPRECATED: This function is no longer used. Project metadata is now embedded in
// the upload payload (v2/metrics). Keeping this function for reference only.
// It will be removed in a future version.
#[allow(dead_code)]
async fn _upload_project_metadata_static_legacy(
    metadata: &crate::project_metadata::ProjectMetadata,
    config: Option<GuideAIConfig>,
) -> Result<(), String> {
    use super::types::ProjectUploadRequest;

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
