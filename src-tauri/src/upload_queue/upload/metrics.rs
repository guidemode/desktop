//! Metrics-only upload without transcript content.
//!
//! Uploads session metadata and metrics, skipping JSONL transcript.

use crate::config::GuideModeConfig;
use crate::database::{
    get_full_session_by_id, get_session_metrics, get_session_rating, SessionMetrics,
};
use crate::logging::{log_info, log_warn};
use crate::project_metadata::extract_project_metadata;
use crate::upload_queue::types::UploadItem;
use chrono::DateTime;
use serde_json::Value;

/// Upload session metadata and metrics only (no JSONL transcript)
pub async fn upload_metrics_only(item: &UploadItem, config: GuideModeConfig) -> Result<(), String> {
    let api_key = config.api_key.clone().ok_or("No API key configured")?;
    let server_url = config
        .server_url
        .clone()
        .ok_or("No server URL configured")?;

    // Get session ID
    let session_id = item
        .session_id
        .as_ref()
        .ok_or("Session ID required for metrics-only sync")?;

    // Fetch full session data from database
    let session_data = get_full_session_by_id(session_id)
        .map_err(|e| format!("Failed to get session data: {}", e))?
        .ok_or_else(|| format!("Session {} not found in database", session_id))?;

    // Extract project metadata if CWD is available (will be embedded in payload)
    let (final_project_name, project_metadata) = if let Some(ref cwd) = item.cwd {
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
                        "✓ Extracted project: {} (type: {}, git: {}) - will embed in payload",
                        metadata.project_name,
                        metadata.detected_project_type,
                        metadata.git_remote_url.as_deref().unwrap_or("none")
                    ),
                )
                .unwrap_or_default();

                // Project metadata will be embedded in the upload payload
                let project_name = metadata.project_name.clone();
                (project_name, Some(metadata))
            }
            Err(e) => {
                log_warn(
                    "upload-queue",
                    &format!(
                        "⚠ Could not extract project metadata: {} - using folder name",
                        e
                    ),
                )
                .unwrap_or_default();
                (item.project_name.clone(), None)
            }
        }
    } else {
        (item.project_name.clone(), None)
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

    // Prepare session upload request without content (metrics only)
    // fileName is included for deduplication (unique constraint on tenant+provider+session+fileName)
    // filePath is intentionally omitted - this signals metrics-only mode to the server
    // fileSize is included as a useful metric for session size analytics
    let mut session_request = serde_json::json!({
        "provider": session_data.provider,
        "projectName": final_project_name,
        "sessionId": session_data.session_id,
        "fileName": session_data.file_name,
        // filePath intentionally omitted for metrics-only uploads
        "fileSize": session_data.file_size,
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

    // Add project metadata if available
    if let Some(ref metadata) = project_metadata {
        session_request["projectMetadata"] = serde_json::json!({
            "gitRemoteUrl": metadata.git_remote_url,
            "cwd": metadata.cwd,
            "detectedProjectType": metadata.detected_project_type,
        });
    }

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
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "Session metadata upload failed (metrics-only mode) with status {}: {}",
            status, error_text
        ));
    }

    log_info(
        "upload-queue",
        &format!("✓ Uploaded session metadata for {}", session_id),
    )
    .unwrap_or_default();

    // Fetch and upload metrics
    if let Ok(Some(metrics)) = get_session_metrics(session_id) {
        upload_session_metrics(&metrics, &server_url, &api_key).await?;
    } else {
        log_warn(
            "upload-queue",
            &format!("⚠ No metrics found for session {}", session_id),
        )
        .unwrap_or_default();
    }

    Ok(())
}

/// Helper function to upload session metrics to server
pub async fn upload_session_metrics(
    metrics: &SessionMetrics,
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

    // Build metrics object in parts to avoid macro recursion limit
    let mut metrics_obj = serde_json::Map::new();

    // Session identifiers
    metrics_obj.insert(
        "sessionId".to_string(),
        serde_json::json!(metrics.session_id),
    );
    metrics_obj.insert("provider".to_string(), serde_json::json!(metrics.provider));

    // Performance metrics
    metrics_obj.insert(
        "responseLatencyMs".to_string(),
        serde_json::json!(metrics.response_latency_ms),
    );
    metrics_obj.insert(
        "taskCompletionTimeMs".to_string(),
        serde_json::json!(metrics.task_completion_time_ms),
    );
    metrics_obj.insert(
        "performanceTotalResponses".to_string(),
        serde_json::json!(metrics.performance_total_responses),
    );

    // Usage metrics
    metrics_obj.insert(
        "readWriteRatio".to_string(),
        serde_json::json!(metrics.read_write_ratio),
    );
    metrics_obj.insert(
        "inputClarityScore".to_string(),
        serde_json::json!(metrics.input_clarity_score),
    );
    metrics_obj.insert(
        "readOperations".to_string(),
        serde_json::json!(metrics.read_operations),
    );
    metrics_obj.insert(
        "writeOperations".to_string(),
        serde_json::json!(metrics.write_operations),
    );
    metrics_obj.insert(
        "totalUserMessages".to_string(),
        serde_json::json!(metrics.total_user_messages),
    );

    // Error metrics
    metrics_obj.insert(
        "errorCount".to_string(),
        serde_json::json!(metrics.error_count),
    );
    metrics_obj.insert(
        "errorTypes".to_string(),
        serde_json::json!(parse_array(&metrics.error_types)),
    );
    metrics_obj.insert(
        "lastErrorMessage".to_string(),
        serde_json::json!(metrics.last_error_message),
    );
    metrics_obj.insert(
        "recoveryAttempts".to_string(),
        serde_json::json!(metrics.recovery_attempts),
    );
    metrics_obj.insert(
        "fatalErrors".to_string(),
        serde_json::json!(metrics.fatal_errors),
    );

    // Engagement metrics
    metrics_obj.insert(
        "interruptionRate".to_string(),
        serde_json::json!(metrics.interruption_rate),
    );
    metrics_obj.insert(
        "sessionLengthMinutes".to_string(),
        serde_json::json!(metrics.session_length_minutes),
    );
    metrics_obj.insert(
        "totalInterruptions".to_string(),
        serde_json::json!(metrics.total_interruptions),
    );
    metrics_obj.insert(
        "engagementTotalResponses".to_string(),
        serde_json::json!(metrics.engagement_total_responses),
    );

    // Quality metrics
    metrics_obj.insert(
        "taskSuccessRate".to_string(),
        serde_json::json!(metrics.task_success_rate),
    );
    metrics_obj.insert(
        "iterationCount".to_string(),
        serde_json::json!(metrics.iteration_count),
    );
    metrics_obj.insert(
        "processQualityScore".to_string(),
        serde_json::json!(metrics.process_quality_score),
    );
    metrics_obj.insert(
        "usedPlanMode".to_string(),
        serde_json::json!(metrics.used_plan_mode),
    );
    metrics_obj.insert(
        "usedTodoTracking".to_string(),
        serde_json::json!(metrics.used_todo_tracking),
    );
    metrics_obj.insert(
        "overTopAffirmations".to_string(),
        serde_json::json!(metrics.over_top_affirmations),
    );
    metrics_obj.insert(
        "successfulOperations".to_string(),
        serde_json::json!(metrics.successful_operations),
    );
    metrics_obj.insert(
        "totalOperations".to_string(),
        serde_json::json!(metrics.total_operations),
    );
    metrics_obj.insert(
        "exitPlanModeCount".to_string(),
        serde_json::json!(metrics.exit_plan_mode_count),
    );
    metrics_obj.insert(
        "todoWriteCount".to_string(),
        serde_json::json!(metrics.todo_write_count),
    );
    metrics_obj.insert(
        "overTopAffirmationsPhrases".to_string(),
        serde_json::json!(parse_array(&metrics.over_top_affirmations_phrases)),
    );
    metrics_obj.insert(
        "improvementTips".to_string(),
        serde_json::json!(parse_array(&metrics.improvement_tips)),
    );

    // Git diff metrics (desktop-only)
    metrics_obj.insert(
        "gitTotalFilesChanged".to_string(),
        serde_json::json!(metrics.git_total_files_changed),
    );
    metrics_obj.insert(
        "gitLinesAdded".to_string(),
        serde_json::json!(metrics.git_lines_added),
    );
    metrics_obj.insert(
        "gitLinesRemoved".to_string(),
        serde_json::json!(metrics.git_lines_removed),
    );
    metrics_obj.insert(
        "gitLinesModified".to_string(),
        serde_json::json!(metrics.git_lines_modified),
    );
    metrics_obj.insert(
        "gitNetLinesChanged".to_string(),
        serde_json::json!(metrics.git_net_lines_changed),
    );
    metrics_obj.insert(
        "gitLinesReadPerLineChanged".to_string(),
        serde_json::json!(metrics.git_lines_read_per_line_changed),
    );
    metrics_obj.insert(
        "gitReadsPerFileChanged".to_string(),
        serde_json::json!(metrics.git_reads_per_file_changed),
    );
    metrics_obj.insert(
        "gitLinesChangedPerMinute".to_string(),
        serde_json::json!(metrics.git_lines_changed_per_minute),
    );
    metrics_obj.insert(
        "gitLinesChangedPerToolUse".to_string(),
        serde_json::json!(metrics.git_lines_changed_per_tool_use),
    );
    metrics_obj.insert(
        "totalLinesRead".to_string(),
        serde_json::json!(metrics.total_lines_read),
    );

    // Custom metrics
    metrics_obj.insert(
        "customMetrics".to_string(),
        serde_json::json!(metrics
            .custom_metrics
            .as_ref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())),
    );

    let metrics_request = serde_json::json!({
        "metrics": [metrics_obj]
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
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "Session metrics upload failed with status {}: {}",
            status, error_text
        ));
    }

    log_info(
        "upload-queue",
        &format!("✓ Uploaded metrics for session {}", metrics.session_id),
    )
    .unwrap_or_default();

    Ok(())
}
