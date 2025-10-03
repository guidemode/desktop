use crate::config::{ensure_logs_dir, get_logs_dir};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{
    fmt::{self},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub provider: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

static LOGGER_INITIALIZED: std::sync::Once = std::sync::Once::new();
use std::sync::LazyLock;
#[allow(dead_code)]
static LOG_WRITERS: LazyLock<Mutex<std::collections::HashMap<String, File>>> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

// Keep the guard alive for the lifetime of the program
static FILE_APPENDER_GUARD: LazyLock<Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>> = LazyLock::new(|| Mutex::new(None));

pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    ensure_logs_dir()?;

    LOGGER_INITIALIZED.call_once(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));

        // Console logging for development - compact format
        let console_layer = fmt::layer()
            .compact()
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_filter(env_filter.clone());

        // File logging for all application output
        let logs_dir = get_logs_dir().expect("Failed to get logs directory");
        let file_appender = tracing_appender::rolling::never(&logs_dir, "app.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        // Store the guard to keep the writer alive
        if let Ok(mut guard_mutex) = FILE_APPENDER_GUARD.lock() {
            *guard_mutex = Some(guard);
        }

        let file_layer = fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_target(true)
            .with_filter(env_filter.clone());

        tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .init();
    });

    Ok(())
}

pub fn log_provider_event(
    provider: &str,
    level: &str,
    message: &str,
    details: Option<serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_logs_dir()?;

    let log_entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        level: level.to_string(),
        provider: provider.to_string(),
        message: message.to_string(),
        details,
    };

    // Log to tracing system
    match level {
        "ERROR" => error!(provider = provider, "{}", message),
        "WARN" => warn!(provider = provider, "{}", message),
        "DEBUG" => debug!(provider = provider, "{}", message),
        _ => info!(provider = provider, "{}", message),
    }

    // Also write to provider-specific file
    write_provider_log_entry(provider, &log_entry)?;

    Ok(())
}

fn write_provider_log_entry(
    provider: &str,
    entry: &LogEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    let logs_dir = get_logs_dir()?;
    let log_file_path = logs_dir.join(format!("{}.log", provider));

    // Check if we need to rotate the log file
    if should_rotate_log(&log_file_path)? {
        rotate_log_file(&log_file_path)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)?;

    // Write JSON entry
    let json_line = serde_json::to_string(entry)?;
    writeln!(file, "{}", json_line)?;
    file.flush()?;

    Ok(())
}

fn should_rotate_log(log_file_path: &PathBuf) -> Result<bool, Box<dyn std::error::Error>> {
    if !log_file_path.exists() {
        return Ok(false);
    }

    let metadata = std::fs::metadata(log_file_path)?;
    const MAX_SIZE: u64 = 10 * 1024 * 1024; // 10MB

    Ok(metadata.len() > MAX_SIZE)
}

fn rotate_log_file(log_file_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Rotate existing backup files (4 -> 5, 3 -> 4, etc.)
    for i in (1..5).rev() {
        let current_backup = log_file_path.with_extension(format!("log.{}", i));
        let next_backup = log_file_path.with_extension(format!("log.{}", i + 1));

        if current_backup.exists() {
            std::fs::rename(&current_backup, &next_backup)?;
        }
    }

    // Move current log to .1
    if log_file_path.exists() {
        let first_backup = log_file_path.with_extension("log.1");
        std::fs::rename(log_file_path, first_backup)?;
    }

    Ok(())
}

fn transform_provider_log_to_claude_format(provider: &str, log_content: &str) -> Option<LogEntry> {
    match provider {
        "app" => transform_app_log(log_content),
        "opencode" => transform_opencode_log(log_content),
        "codex" => transform_codex_log(log_content),
        _ => None,
    }
}

fn transform_app_log(log_content: &str) -> Option<LogEntry> {
    // Parse tracing format: "2025-10-03T19:29:51.226423Z  INFO guideai_desktop::logging: message provider=\"name\""
    let parts: Vec<&str> = log_content.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return None;
    }

    let timestamp = parts[0].trim();
    let level = parts[1].trim();

    // Find the colon that separates the target from the message
    let rest = parts[2..].join(" ");
    let colon_pos = rest.find(':')?;
    let message_part = rest[colon_pos + 1..].trim();

    // Extract provider from message if it exists (provider="name" format)
    let provider = if let Some(provider_start) = message_part.find("provider=\"") {
        let start = provider_start + 10; // length of 'provider="'
        if let Some(end) = message_part[start..].find('"') {
            message_part[start..start + end].to_string()
        } else {
            "app".to_string()
        }
    } else {
        "app".to_string()
    };

    // Remove provider tag from message
    let clean_message = if let Some(provider_pos) = message_part.find("provider=") {
        message_part[..provider_pos].trim().to_string()
    } else {
        message_part.to_string()
    };

    Some(LogEntry {
        timestamp: timestamp.to_string(),
        level: level.to_uppercase(),
        provider,
        message: clean_message,
        details: None,
    })
}

fn transform_opencode_log(log_content: &str) -> Option<LogEntry> {
    use serde_json::Value;

    // Try to parse as OpenCode project record
    if let Ok(value) = serde_json::from_str::<Value>(log_content) {
        if let Some(worktree) = value.get("worktree").and_then(|w| w.as_str()) {
            // This is an OpenCode project record
            let project_name = std::path::Path::new(worktree)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let timestamp = value.get("time")
                .and_then(|t| t.get("updated").or_else(|| t.get("created")))
                .and_then(|ts| ts.as_i64())
                .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts))
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339());

            return Some(LogEntry {
                timestamp,
                level: "INFO".to_string(),
                provider: "opencode".to_string(),
                message: format!("Project: {} at {}", project_name, worktree),
                details: Some(value),
            });
        }
    }
    None
}

fn transform_codex_log(log_content: &str) -> Option<LogEntry> {
    use serde_json::Value;

    // Try to parse as Codex log entry
    if let Ok(value) = serde_json::from_str::<Value>(log_content) {
        let timestamp = value.get("timestamp")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        let entry_type = value.get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");

        let message = match entry_type {
            "session_meta" => {
                if let Some(payload) = value.get("payload") {
                    let session_id = payload.get("id").and_then(|id| id.as_str()).unwrap_or("unknown");
                    let cwd = payload.get("cwd").and_then(|c| c.as_str()).unwrap_or("unknown");
                    format!("Session started: {} in {}", session_id, cwd)
                } else {
                    "Session metadata".to_string()
                }
            },
            "response_item" => {
                if let Some(payload) = value.get("payload") {
                    let role = payload.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
                    format!("Message from {}", role)
                } else {
                    "Response item".to_string()
                }
            },
            _ => format!("Codex event: {}", entry_type),
        };

        return Some(LogEntry {
            timestamp,
            level: "INFO".to_string(),
            provider: "codex".to_string(),
            message,
            details: Some(value),
        });
    }
    None
}

pub fn read_provider_logs(
    provider: &str,
    max_lines: Option<usize>,
) -> Result<Vec<LogEntry>, Box<dyn std::error::Error>> {
    let logs_dir = get_logs_dir()?;
    let log_file_path = logs_dir.join(format!("{}.log", provider));

    if !log_file_path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(&log_file_path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        match line {
            Ok(line_content) => {
                // Try to parse as Claude format first
                if let Ok(entry) = serde_json::from_str::<LogEntry>(&line_content) {
                    entries.push(entry);
                } else {
                    // If it fails, try to transform from provider-specific format
                    if let Some(transformed_entry) = transform_provider_log_to_claude_format(provider, &line_content) {
                        entries.push(transformed_entry);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading log line: {}", e);
            }
        }
    }

    // Always return in reverse chronological order (newest first)
    entries.reverse();

    // If max_lines is specified, return only the first N entries (which are now the newest)
    if let Some(max) = max_lines {
        if entries.len() > max {
            entries.truncate(max);
        }
    }

    Ok(entries)
}

// Convenience functions for different log levels
pub fn log_debug(provider: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    log_provider_event(provider, "DEBUG", message, None)
}

pub fn log_info(provider: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    log_provider_event(provider, "INFO", message, None)
}

pub fn log_warn(provider: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    log_provider_event(provider, "WARN", message, None)
}

pub fn log_error(provider: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    log_provider_event(provider, "ERROR", message, None)
}

#[allow(dead_code)]
pub fn log_with_details(
    provider: &str,
    level: &str,
    message: &str,
    details: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    log_provider_event(provider, level, message, Some(details))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_log_rotation() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("test.log");

        // Create a file larger than rotation threshold
        {
            let mut file = File::create(&log_file).unwrap();
            let large_content = "x".repeat(11 * 1024 * 1024); // 11MB
            file.write_all(large_content.as_bytes()).unwrap();
        }

        assert!(should_rotate_log(&log_file).unwrap());

        rotate_log_file(&log_file).unwrap();

        let backup_file = log_file.with_extension("log.1");
        assert!(backup_file.exists());
        assert!(!log_file.exists());
    }

    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            level: "INFO".to_string(),
            provider: "claude-code".to_string(),
            message: "Test message".to_string(),
            details: Some(serde_json::json!({"key": "value"})),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: LogEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.timestamp, parsed.timestamp);
        assert_eq!(entry.level, parsed.level);
        assert_eq!(entry.provider, parsed.provider);
        assert_eq!(entry.message, parsed.message);
    }
}