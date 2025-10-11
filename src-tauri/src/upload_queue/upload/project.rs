//! Project metadata upload.
//!
//! **DEPRECATED**: Project metadata should now be embedded in session upload payloads
//! (v2 and metrics uploads) instead of being uploaded separately. This reduces API calls
//! and ensures atomic session+project updates.
//!
//! This module is kept for backward compatibility and legacy code paths only.

use crate::config::GuideAIConfig;
use crate::logging::log_info;
use crate::project_metadata::ProjectMetadata;
use crate::upload_queue::types::ProjectUploadRequest;

/// Upload project metadata to the server (static version for use in async tasks)
///
/// **DEPRECATED**: Use embedded `projectMetadata` in v2/metrics upload payloads instead.
/// This function makes a separate API call which is inefficient and can cause duplicate
/// project metadata uploads. New code should embed project metadata directly in the
/// session upload payload.
#[deprecated(
    since = "0.1.21",
    note = "Embed projectMetadata in upload payloads instead of separate /api/projects calls"
)]
pub async fn upload_project_metadata_static(
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
