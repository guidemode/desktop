//! Metrics-only upload without transcript content.
//!
//! Uploads session metadata and metrics, skipping JSONL transcript.

use crate::config::GuideAIConfig;
use crate::database::{get_full_session_by_id, get_session_metrics, get_session_rating, SessionMetrics};
use crate::logging::{log_info, log_warn};
use crate::project_metadata::extract_project_metadata;
use crate::upload_queue::types::UploadItem;
use chrono::DateTime;
use serde_json::Value;

/// Upload session metadata and metrics only (no JSONL transcript)
pub async fn upload_metrics_only(
    item: &UploadItem,
    config: GuideAIConfig,
) -> Result<(), String> {
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
                    &format!("⚠ Could not extract project metadata: {} - using folder name", e),
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
