//! Upload coordination and routing.
//!
//! Routes upload requests to appropriate handlers (v2, metrics, project).

// Upload submodules
pub mod metrics;
pub mod project;
pub mod retry;
pub mod v2;

// Re-export main functions
pub use metrics::upload_metrics_only;
pub use v2::upload_v2;

// Re-export deprecated function for backward compatibility
#[allow(deprecated)]
pub use project::upload_project_metadata_static;

// Re-export retry utilities
pub use retry::{calculate_backoff, classify_error, schedule_retry, should_retry, ErrorType};

use crate::config::GuideModeConfig;
use crate::upload_queue::hashing::{calculate_content_hash_sha256, calculate_file_hash_sha256};
use crate::upload_queue::types::UploadItem;

/// Process an upload item by routing to the appropriate upload method based on sync mode
pub async fn process_upload_item(
    item: &UploadItem,
    config: Option<GuideModeConfig>,
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
            upload_metrics_only(item, config.clone()).await
        }
        "Transcript and Metrics" => {
            // Full sync: use v2 upload with compression and deduplication

            // Extract session ID
            let session_id = item.session_id.as_ref()
                .ok_or("Session ID required for upload")?;

            // Calculate file hash if not already present
            let file_hash = if let Some(ref hash) = item.file_hash {
                hash.clone()
            } else {
                // Calculate SHA256 hash
                if let Some(ref content) = item.content {
                    calculate_content_hash_sha256(content)
                } else {
                    calculate_file_hash_sha256(&item.file_path)?
                }
            };

            // Use v2 upload endpoint
            upload_v2(item, session_id, &file_hash, config.clone()).await
        }
        _ => {
            Err(format!(
                "Sync mode is '{}', skipping upload (expected 'Metrics Only' or 'Transcript and Metrics')",
                provider_config.sync_mode
            ))
        }
    }
}
