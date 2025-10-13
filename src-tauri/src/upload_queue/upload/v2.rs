//! V2 upload with compression and deduplication.
//!
//! Uploads full session content with gzip compression and hash-based deduplication.

use crate::config::GuideAIConfig;
use crate::database::{get_full_session_by_id, get_session_metrics, get_session_rating};
use crate::logging::log_info;
use crate::project_metadata::extract_project_metadata;
use crate::upload_queue::types::UploadItem;
use crate::upload_queue::compression::compress_file_content;
use chrono::DateTime;
use serde_json::Value;

/// Check if file hash exists on server (v2 upload optimization)
pub async fn check_file_hash(
    session_id: &str,
    file_hash: &str,
    server_url: &str,
    api_key: &str,
) -> Result<bool, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/agent-sessions/check-hash?sessionId={}&fileHash={}",
        server_url, session_id, file_hash
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Failed to check hash: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "Hash check failed with status {}: {}",
            status, error_text
        ));
    }

    #[derive(serde::Deserialize)]
    struct HashCheckResponse {
        #[serde(rename = "needsUpload")]
        needs_upload: bool,
    }

    let hash_response: HashCheckResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse hash check response: {}", e))?;

    Ok(hash_response.needs_upload)
}

/// Upload session using v2 endpoint with compression and deduplication
pub async fn upload_v2(
    item: &UploadItem,
    session_id: &str,
    file_hash: &str,
    config: GuideAIConfig,
) -> Result<(), String> {
    let api_key = config.api_key.clone().ok_or("No API key configured")?;
    let server_url = config.server_url.clone().ok_or("No server URL configured")?;

    // Check if server already has this file
    let needs_upload = check_file_hash(session_id, file_hash, &server_url, &api_key).await?;

    // Prepare content only if needed
    let compressed_content = if needs_upload {
        // Read file content
        let file_content = if let Some(ref content) = item.content {
            content.as_bytes().to_vec()
        } else {
            std::fs::read(&item.file_path)
                .map_err(|e| format!("Failed to read file: {}", e))?
        };

        // Compress the file content
        let compressed = compress_file_content(&file_content)?;

        // Encode compressed content to base64
        use base64::Engine;
        Some(base64::engine::general_purpose::STANDARD.encode(&compressed))
    } else {
        log_info(
            "upload-queue",
            &format!("⚡ File hash matches, skipping content upload for {}", session_id),
        )
        .unwrap_or_default();
        None
    };

    // Get full session data from database
    let session_data = get_full_session_by_id(session_id)
        .map_err(|e| format!("Failed to get session data: {}", e))?
        .ok_or_else(|| format!("Session {} not found in database", session_id))?;

    // Get metrics if available
    let metrics = get_session_metrics(session_id).ok().flatten();

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

    // Extract project metadata if CWD is available (will be embedded in payload)
    let (final_project_name, project_metadata) = if let Some(ref cwd) = item.cwd {
        match extract_project_metadata(cwd) {
            Ok(metadata) => {
                // Project metadata will be embedded in the upload payload
                let project_name = metadata.project_name.clone();
                (project_name, Some(metadata))
            }
            Err(_) => (item.project_name.clone(), None),
        }
    } else {
        (item.project_name.clone(), None)
    };

    // Prepare upload request with embedded metrics and project metadata
    let mut upload_request = serde_json::json!({
        "provider": session_data.provider,
        "projectName": final_project_name,
        "sessionId": session_data.session_id,
        "fileName": session_data.file_name,
        "filePath": session_data.file_path,
        "fileHash": file_hash,
        "fileSize": item.file_size,
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
        "gitBranch": session_data.git_branch,
        "firstCommitHash": session_data.first_commit_hash,
        "latestCommitHash": session_data.latest_commit_hash,
    });

    // Add project metadata if available
    if let Some(ref metadata) = project_metadata {
        upload_request["projectMetadata"] = serde_json::json!({
            "gitRemoteUrl": metadata.git_remote_url,
            "cwd": metadata.cwd,
            "detectedProjectType": metadata.detected_project_type,
        });
    }

    // Add compressed content if needed
    if let Some(content) = compressed_content {
        upload_request["content"] = serde_json::json!(content);
        upload_request["contentEncoding"] = serde_json::json!("gzip");
    }

    // Add metrics if available
    if let Some(ref m) = metrics {
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

        upload_request["metrics"] = serde_json::json!({
            "sessionId": m.session_id,
            "provider": m.provider,
            // Performance metrics
            "responseLatencyMs": m.response_latency_ms,
            "taskCompletionTimeMs": m.task_completion_time_ms,
            "performanceTotalResponses": m.performance_total_responses,
            // Usage metrics
            "readWriteRatio": m.read_write_ratio,
            "inputClarityScore": m.input_clarity_score,
            "readOperations": m.read_operations,
            "writeOperations": m.write_operations,
            "totalUserMessages": m.total_user_messages,
            // Error metrics
            "errorCount": m.error_count,
            "errorTypes": parse_array(&m.error_types),
            "lastErrorMessage": m.last_error_message,
            "recoveryAttempts": m.recovery_attempts,
            "fatalErrors": m.fatal_errors,
            // Engagement metrics
            "interruptionRate": m.interruption_rate,
            "sessionLengthMinutes": m.session_length_minutes,
            "totalInterruptions": m.total_interruptions,
            "engagementTotalResponses": m.engagement_total_responses,
            // Quality metrics
            "taskSuccessRate": m.task_success_rate,
            "iterationCount": m.iteration_count,
            "processQualityScore": m.process_quality_score,
            "usedPlanMode": m.used_plan_mode,
            "usedTodoTracking": m.used_todo_tracking,
            "overTopAffirmations": m.over_top_affirmations,
            "successfulOperations": m.successful_operations,
            "totalOperations": m.total_operations,
            "exitPlanModeCount": m.exit_plan_mode_count,
            "todoWriteCount": m.todo_write_count,
            "overTopAffirmationsPhrases": parse_array(&m.over_top_affirmations_phrases),
            "improvementTips": parse_array(&m.improvement_tips),
            // Git diff metrics (desktop-only)
            "gitTotalFilesChanged": m.git_total_files_changed,
            "gitLinesAdded": m.git_lines_added,
            "gitLinesRemoved": m.git_lines_removed,
            "gitLinesModified": m.git_lines_modified,
            "gitNetLinesChanged": m.git_net_lines_changed,
            "gitLinesReadPerLineChanged": m.git_lines_read_per_line_changed,
            "gitReadsPerFileChanged": m.git_reads_per_file_changed,
            "gitLinesChangedPerMinute": m.git_lines_changed_per_minute,
            "gitLinesChangedPerToolUse": m.git_lines_changed_per_tool_use,
            "totalLinesRead": m.total_lines_read,
            // Custom metrics
            "customMetrics": m.custom_metrics.as_ref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        });
    }

    // Make HTTP request to v2 endpoint
    let client = reqwest::Client::new();
    let url = format!("{}/api/agent-sessions/upload-v2", server_url);

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
            "Upload v2 failed with status {}: {}",
            status, error_text
        ));
    }

    log_info(
        "upload-queue",
        &format!("✓ Uploaded session via v2 for {}", session_id),
    )
    .unwrap_or_default();

    Ok(())
}
