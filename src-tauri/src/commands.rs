use crate::auth_server::{AuthError, AuthServer};
use crate::config::{
    clear_config, delete_provider_config, ensure_logs_dir, load_config, load_provider_config,
    save_config, save_provider_config, ActivityLogEntry, GuideModeConfig, ProjectInfo,
    ProviderConfig,
};
use crate::logging::{read_provider_logs, LogEntry};
use crate::providers::{
    ClaudeWatcher, ClaudeWatcherStatus, CodexWatcher, CodexWatcherStatus,
    CopilotWatcher, CopilotWatcherStatus, CursorWatcher, CursorWatcherStatus, GeminiWatcher,
    GeminiWatcherStatus, OpenCodeWatcher, OpenCodeWatcherStatus, SessionInfo,
};
use crate::upload_queue::{QueueItems, UploadQueue, UploadStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::State;

#[tauri::command]
pub async fn load_config_command() -> Result<GuideModeConfig, String> {
    load_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_config_command(config: GuideModeConfig) -> Result<(), String> {
    save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_config_command() -> Result<(), String> {
    clear_config().map_err(|e| e.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionResponse {
    user: UserInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserInfo {
    username: String,
    name: Option<String>,
    #[serde(rename = "avatarUrl")]
    avatar_url: Option<String>,
}

#[tauri::command]
pub async fn login_command(
    server_url: String,
    _app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Start the auth server - this handles automatic port selection and cleanup
    let (auth_server, result_rx) = AuthServer::start()
        .await
        .map_err(|e| format!("Failed to start authentication server: {}", e))?;

    let callback_url = &auth_server.callback_url;
    let auth_url = format!(
        "{}/auth/desktop?redirect_uri={}",
        server_url,
        urlencoding::encode(callback_url)
    );

    // Log server details for debugging
    use tracing::info;
    info!(
        port = auth_server.port,
        callback_url = %callback_url,
        auth_url = %auth_url,
        "Authentication server started"
    );

    // Authentication flow with guaranteed cleanup
    let result = async {
        // Open the browser to the OAuth URL
        open::that(&auth_url).map_err(|e| format!("Failed to open browser: {}", e))?;

        // Wait for callback with 5-minute timeout (matching CLI behavior)
        let auth_data =
            AuthServer::wait_for_callback_with_timeout(result_rx, Duration::from_secs(300))
                .await
                .map_err(|e| match e {
                    AuthError::TimeoutError => {
                        "Authentication timed out after 5 minutes. Please try again.".to_string()
                    }
                    AuthError::CallbackError(msg) => format!("Authentication failed: {}", msg),
                    _ => format!("Authentication error: {}", e),
                })?;

        // Verify the credentials by calling the session endpoint
        info!(server_url = %server_url, "Verifying session with server");
        let user_info = verify_session(&server_url, &auth_data.api_key)
            .await
            .map_err(|e| format!("Failed to verify credentials: {}", e))?;
        info!(username = %user_info.username, "Session verified successfully");

        // Save the complete configuration
        let config = GuideModeConfig {
            api_key: Some(auth_data.api_key.clone()),
            server_url: Some(server_url.clone()),
            username: Some(user_info.username.clone()),
            name: user_info.name.clone(),
            avatar_url: user_info.avatar_url.clone(),
            tenant_id: Some(auth_data.tenant_id.clone()),
            tenant_name: Some(auth_data.tenant_name.clone()),
        };

        info!("Saving authentication configuration");
        save_config(&config).map_err(|e| format!("Failed to save configuration: {}", e))?;
        info!("Configuration saved successfully");

        // Update upload queue with new config
        state.upload_queue.set_config(config);
        info!("Upload queue configuration updated");

        Ok::<(), String>(())
    }
    .await;

    // ALWAYS shutdown the server, regardless of success or failure
    auth_server.shutdown().await;

    result
}

async fn verify_session(
    server_url: &str,
    api_key: &str,
) -> Result<UserInfo, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let url = format!("{}/auth/session", server_url);

    println!("Making request to: {}", url);
    println!("Using API key: {}...", &api_key[..20]); // Only show first 20 chars for security

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    let status = response.status();
    println!("Response status: {}", status);

    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read error response".to_string());
        println!("Error response body: {}", error_text);
        return Err(format!(
            "Session verification failed with status: {} - {}",
            status, error_text
        )
        .into());
    }

    let response_text = response.text().await?;
    use tracing::debug;
    debug!(response_body = %response_text, "Session verification response");

    let session: SessionResponse = serde_json::from_str(&response_text)?;
    Ok(session.user)
}

#[tauri::command]
pub async fn logout_command(state: State<'_, AppState>) -> Result<(), String> {
    clear_config_command().await?;

    // Clear upload queue config by setting an empty config
    let empty_config = GuideModeConfig {
        api_key: None,
        server_url: None,
        username: None,
        name: None,
        avatar_url: None,
        tenant_id: None,
        tenant_name: None,
    };
    state.upload_queue.set_config(empty_config);
    use tracing::info;
    info!("Upload queue configuration cleared");

    Ok(())
}

// Provider config commands
#[tauri::command]
pub async fn load_provider_config_command(provider_id: String) -> Result<ProviderConfig, String> {
    load_provider_config(&provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_provider_config_command(
    provider_id: String,
    config: ProviderConfig,
) -> Result<(), String> {
    save_provider_config(&provider_id, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_provider_config_command(provider_id: String) -> Result<(), String> {
    delete_provider_config(&provider_id).map_err(|e| e.to_string())
}

// Setup instructions command
#[tauri::command]
pub async fn load_setup_instructions_command(file_name: String) -> Result<String, String> {
    // Setup instructions are bundled as resources at compile time using include_str! macro
    // This embeds the files directly into the binary at build time
    match file_name.as_str() {
        "gemini-code.md" => Ok(include_str!("../../setup-instructions/gemini-code.md").to_string()),
        "claude-code.md" => Ok(include_str!("../../setup-instructions/claude-code.md").to_string()),
        "github-copilot.md" => {
            Ok(include_str!("../../setup-instructions/github-copilot.md").to_string())
        }
        "opencode.md" => Ok(include_str!("../../setup-instructions/opencode.md").to_string()),
        "codex.md" => Ok(include_str!("../../setup-instructions/codex.md").to_string()),
        _ => Err(format!("Unknown setup instructions file: {}", file_name)),
    }
}

// Project scanning commands
#[tauri::command]
pub async fn scan_projects_command(
    provider_id: String,
    directory: String,
) -> Result<Vec<ProjectInfo>, String> {
    crate::providers::scan_projects(&provider_id, &directory)
}

// Directory validation command
#[tauri::command]
pub async fn check_directory_exists(path: String) -> Result<bool, String> {
    use std::path::Path;

    // Expand ~ and environment variables
    let expanded = shellexpand::tilde(&path).to_string();
    Ok(Path::new(&expanded).exists())
}

// Activity logging commands
#[tauri::command]
pub async fn add_activity_log_command(entry: ActivityLogEntry) -> Result<(), String> {
    ensure_logs_dir().map_err(|e| e.to_string())?;

    let logs_dir = crate::config::get_logs_dir().map_err(|e| e.to_string())?;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let log_file = logs_dir.join(format!("{}.jsonl", today));

    let log_line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    let log_entry = format!("{}\n", log_line);

    use std::io::Write;
    if log_file.exists() {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&log_file)
            .map_err(|e| e.to_string())?;
        file.write_all(log_entry.as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        fs::write(&log_file, log_entry).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_activity_logs_command(
    limit: Option<usize>,
) -> Result<Vec<ActivityLogEntry>, String> {
    let logs_dir = crate::config::get_logs_dir().map_err(|e| e.to_string())?;

    if !logs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut all_logs = Vec::new();

    // Read log files from most recent to oldest
    let mut log_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                log_files.push(path);
            }
        }
    }

    log_files.sort_by(|a, b| b.cmp(a)); // Reverse sort for most recent first

    for log_file in log_files {
        if let Ok(content) = fs::read_to_string(&log_file) {
            for line in content.lines().rev() {
                // Reverse lines to get most recent first
                if let Ok(entry) = serde_json::from_str::<ActivityLogEntry>(line) {
                    all_logs.push(entry);
                    if let Some(limit) = limit {
                        if all_logs.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        if let Some(limit) = limit {
            if all_logs.len() >= limit {
                break;
            }
        }
    }

    Ok(all_logs)
}

// Application state for managing watchers and upload queue
#[derive(Debug)]
pub enum Watcher {
    Claude(ClaudeWatcher),
    Copilot(CopilotWatcher),
    OpenCode(OpenCodeWatcher),
    Codex(CodexWatcher),
    Gemini(GeminiWatcher),
    Cursor(CursorWatcher),
}

impl Watcher {
    pub fn stop(&self) {
        match self {
            Watcher::Claude(watcher) => watcher.stop(),
            Watcher::Copilot(watcher) => watcher.stop(),
            Watcher::OpenCode(watcher) => watcher.stop(),
            Watcher::Codex(watcher) => watcher.stop(),
            Watcher::Gemini(watcher) => watcher.stop(),
            Watcher::Cursor(watcher) => { let _ = watcher.stop(); },
        }
    }
}

pub struct AppState {
    pub watchers: Arc<Mutex<HashMap<String, Watcher>>>,
    pub upload_queue: Arc<UploadQueue>,
    pub event_bus: crate::events::EventBus,
}

impl AppState {
    pub fn new(event_bus: crate::events::EventBus) -> Self {
        let upload_queue = Arc::new(UploadQueue::new());

        // Start the upload queue processor
        if let Err(e) = upload_queue.start_processing() {
            eprintln!("Failed to start upload queue processor: {}", e);
        }

        Self {
            watchers: Arc::new(Mutex::new(HashMap::new())),
            upload_queue,
            event_bus,
        }
    }
}

// Claude watcher commands
#[tauri::command]
pub async fn start_claude_watcher(
    state: State<'_, AppState>,
    projects: Vec<String>,
) -> Result<(), String> {
    // Load provider config to check home directory
    let provider_config = load_provider_config("claude-code")
        .map_err(|e| format!("Failed to load provider config: {}", e))?;

    // Check if home directory exists before starting watcher
    let expanded_path = shellexpand::tilde(&provider_config.home_directory).to_string();
    if !std::path::Path::new(&expanded_path).exists() {
        return Err(format!(
            "Cannot start watcher: directory '{}' does not exist. Please install Claude Code or configure a valid directory.",
            provider_config.home_directory
        ));
    }

    // Update upload queue with current config
    if let Ok(config) = load_config() {
        state.upload_queue.set_config(config);
    }

    // Create new watcher
    let watcher = ClaudeWatcher::new(
        projects,
        Arc::clone(&state.upload_queue),
        state.event_bus.clone(),
    )
    .map_err(|e| format!("Failed to create Claude watcher: {}", e))?;

    // Store watcher in state
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.insert("claude-code".to_string(), Watcher::Claude(watcher));
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_claude_watcher(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut watchers) = state.watchers.lock() {
        if let Some(watcher) = watchers.remove("claude-code") {
            watcher.stop();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_claude_watcher_status(
    state: State<'_, AppState>,
) -> Result<ClaudeWatcherStatus, String> {
    if let Ok(watchers) = state.watchers.lock() {
        if let Some(Watcher::Claude(watcher)) = watchers.get("claude-code") {
            Ok(watcher.get_status())
        } else {
            Ok(ClaudeWatcherStatus {
                is_running: false,
                pending_uploads: 0,
                processing_uploads: 0,
                failed_uploads: 0,
            })
        }
    } else {
        Err("Failed to access watcher state".to_string())
    }
}

// OpenCode watcher commands
#[tauri::command]
pub async fn start_opencode_watcher(
    state: State<'_, AppState>,
    projects: Vec<String>,
) -> Result<(), String> {
    // Load provider config to check home directory
    let provider_config = load_provider_config("opencode")
        .map_err(|e| format!("Failed to load provider config: {}", e))?;

    // Check if home directory exists before starting watcher
    let expanded_path = shellexpand::tilde(&provider_config.home_directory).to_string();
    if !std::path::Path::new(&expanded_path).exists() {
        return Err(format!(
            "Cannot start watcher: directory '{}' does not exist. Please install OpenCode or configure a valid directory.",
            provider_config.home_directory
        ));
    }

    // Update upload queue with current config
    if let Ok(config) = load_config() {
        state.upload_queue.set_config(config);
    }

    // Create new watcher
    let watcher = OpenCodeWatcher::new(
        projects,
        Arc::clone(&state.upload_queue),
        state.event_bus.clone(),
    )
    .map_err(|e| format!("Failed to create OpenCode watcher: {}", e))?;

    // Store watcher in state
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.insert("opencode".to_string(), Watcher::OpenCode(watcher));
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_opencode_watcher(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut watchers) = state.watchers.lock() {
        if let Some(watcher) = watchers.remove("opencode") {
            watcher.stop();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_opencode_watcher_status(
    state: State<'_, AppState>,
) -> Result<OpenCodeWatcherStatus, String> {
    if let Ok(watchers) = state.watchers.lock() {
        if let Some(Watcher::OpenCode(watcher)) = watchers.get("opencode") {
            Ok(watcher.get_status())
        } else {
            Ok(OpenCodeWatcherStatus {
                is_running: false,
                pending_uploads: 0,
                processing_uploads: 0,
                failed_uploads: 0,
            })
        }
    } else {
        Err("Failed to access watcher state".to_string())
    }
}

// Codex watcher commands
#[tauri::command]
pub async fn start_codex_watcher(
    state: State<'_, AppState>,
    projects: Vec<String>,
) -> Result<(), String> {
    // Load provider config to check home directory
    let provider_config = load_provider_config("codex")
        .map_err(|e| format!("Failed to load provider config: {}", e))?;

    // Check if home directory exists before starting watcher
    let expanded_path = shellexpand::tilde(&provider_config.home_directory).to_string();
    if !std::path::Path::new(&expanded_path).exists() {
        return Err(format!(
            "Cannot start watcher: directory '{}' does not exist. Please install Codex or configure a valid directory.",
            provider_config.home_directory
        ));
    }

    // Update upload queue with current config
    if let Ok(config) = load_config() {
        state.upload_queue.set_config(config);
    }

    // Create new watcher
    let watcher = CodexWatcher::new(
        projects,
        Arc::clone(&state.upload_queue),
        state.event_bus.clone(),
    )
    .map_err(|e| format!("Failed to create Codex watcher: {}", e))?;

    // Store watcher in state
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.insert("codex".to_string(), Watcher::Codex(watcher));
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_codex_watcher(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut watchers) = state.watchers.lock() {
        if let Some(watcher) = watchers.remove("codex") {
            watcher.stop();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_codex_watcher_status(
    state: State<'_, AppState>,
) -> Result<CodexWatcherStatus, String> {
    if let Ok(watchers) = state.watchers.lock() {
        if let Some(Watcher::Codex(watcher)) = watchers.get("codex") {
            Ok(watcher.get_status())
        } else {
            Ok(CodexWatcherStatus {
                is_running: false,
                pending_uploads: 0,
                processing_uploads: 0,
                failed_uploads: 0,
            })
        }
    } else {
        Err("Failed to access watcher state".to_string())
    }
}

// Cursor watcher commands
#[tauri::command]
pub async fn start_cursor_watcher(
    state: State<'_, AppState>,
    projects: Vec<String>,
) -> Result<(), String> {
    // Check if chats directory exists
    let chats_path = shellexpand::tilde("~/.cursor/chats").to_string();
    if !std::path::Path::new(&chats_path).exists() {
        return Err(format!(
            "Cannot start watcher: Cursor chats directory '{}' does not exist. Please install Cursor or create a session.",
            chats_path
        ));
    }

    let upload_queue = Arc::clone(&state.upload_queue);
    let event_bus = state.event_bus.clone();

    match CursorWatcher::new(projects, upload_queue, event_bus) {
        Ok(watcher) => {
            if let Ok(mut watchers) = state.watchers.lock() {
                watchers.insert("cursor".to_string(), Watcher::Cursor(watcher));
            }
            Ok(())
        }
        Err(e) => Err(format!("Failed to start Cursor watcher: {}", e)),
    }
}

#[tauri::command]
pub async fn stop_cursor_watcher(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut watchers) = state.watchers.lock() {
        if let Some(watcher) = watchers.remove("cursor") {
            watcher.stop();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_cursor_watcher_status(
    state: State<'_, AppState>,
) -> Result<CursorWatcherStatus, String> {
    if let Ok(watchers) = state.watchers.lock() {
        if let Some(Watcher::Cursor(watcher)) = watchers.get("cursor") {
            watcher.get_status()
        } else {
            Ok(CursorWatcherStatus::default())
        }
    } else {
        Err("Failed to access watcher state".to_string())
    }
}

// Copilot watcher commands
#[tauri::command]
pub async fn start_copilot_watcher(
    state: State<'_, AppState>,
    projects: Vec<String>,
) -> Result<(), String> {
    // Load provider config to check home directory
    let provider_config = load_provider_config("github-copilot")
        .map_err(|e| format!("Failed to load provider config: {}", e))?;

    // Check if home directory exists before starting watcher
    let expanded_path = shellexpand::tilde(&provider_config.home_directory).to_string();
    if !std::path::Path::new(&expanded_path).exists() {
        return Err(format!(
            "Cannot start watcher: directory '{}' does not exist. Please install GitHub Copilot or configure a valid directory.",
            provider_config.home_directory
        ));
    }

    // Update upload queue with current config
    if let Ok(config) = load_config() {
        state.upload_queue.set_config(config);
    }

    // Create new watcher
    let watcher = CopilotWatcher::new(
        projects,
        Arc::clone(&state.upload_queue),
        state.event_bus.clone(),
    )
    .map_err(|e| format!("Failed to create Copilot watcher: {}", e))?;

    // Store watcher in state
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.insert("github-copilot".to_string(), Watcher::Copilot(watcher));
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_copilot_watcher(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut watchers) = state.watchers.lock() {
        if let Some(watcher) = watchers.remove("github-copilot") {
            watcher.stop();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_copilot_watcher_status(
    state: State<'_, AppState>,
) -> Result<CopilotWatcherStatus, String> {
    if let Ok(watchers) = state.watchers.lock() {
        if let Some(Watcher::Copilot(watcher)) = watchers.get("github-copilot") {
            Ok(watcher.get_status())
        } else {
            Ok(CopilotWatcherStatus {
                is_running: false,
                pending_uploads: 0,
                processing_uploads: 0,
                failed_uploads: 0,
            })
        }
    } else {
        Err("Failed to access watcher state".to_string())
    }
}

// Gemini watcher commands
#[tauri::command]
pub async fn start_gemini_watcher(
    state: State<'_, AppState>,
    projects: Vec<String>,
) -> Result<(), String> {
    // Load provider config to check home directory
    let provider_config = load_provider_config("gemini-code")
        .map_err(|e| format!("Failed to load provider config: {}", e))?;

    // Check if home directory exists before starting watcher
    let expanded_path = shellexpand::tilde(&provider_config.home_directory).to_string();
    if !std::path::Path::new(&expanded_path).exists() {
        return Err(format!(
            "Cannot start watcher: directory '{}' does not exist. Please install Gemini Code or configure a valid directory.",
            provider_config.home_directory
        ));
    }

    // Update upload queue with current config
    if let Ok(config) = load_config() {
        state.upload_queue.set_config(config);
    }

    // Create new watcher - projects parameter now contains hashes (not CWDs)
    let watcher = GeminiWatcher::new(
        projects,
        Arc::clone(&state.upload_queue),
        state.event_bus.clone(),
    )
    .map_err(|e| format!("Failed to create Gemini watcher: {}", e))?;

    // Store watcher in state
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.insert("gemini-code".to_string(), Watcher::Gemini(watcher));
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_gemini_watcher(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut watchers) = state.watchers.lock() {
        if let Some(watcher) = watchers.remove("gemini-code") {
            watcher.stop();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_gemini_watcher_status(
    state: State<'_, AppState>,
) -> Result<GeminiWatcherStatus, String> {
    if let Ok(watchers) = state.watchers.lock() {
        if let Some(Watcher::Gemini(watcher)) = watchers.get("gemini-code") {
            Ok(watcher.get_status())
        } else {
            Ok(GeminiWatcherStatus {
                is_running: false,
                pending_uploads: 0,
                processing_uploads: 0,
                failed_uploads: 0,
            })
        }
    } else {
        Err("Failed to access watcher state".to_string())
    }
}

#[tauri::command]
pub async fn get_upload_queue_status(state: State<'_, AppState>) -> Result<UploadStatus, String> {
    Ok(state.upload_queue.get_status())
}

#[tauri::command]
pub async fn retry_failed_uploads(state: State<'_, AppState>) -> Result<(), String> {
    state.upload_queue.retry_failed();
    Ok(())
}

#[tauri::command]
pub async fn clear_failed_uploads(state: State<'_, AppState>) -> Result<(), String> {
    state.upload_queue.clear_failed();
    Ok(())
}

#[tauri::command]
pub async fn get_upload_queue_items(state: State<'_, AppState>) -> Result<QueueItems, String> {
    Ok(state.upload_queue.get_all_items())
}

#[tauri::command]
pub async fn retry_single_upload(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<(), String> {
    state.upload_queue.retry_item(&item_id)
}

#[tauri::command]
pub async fn remove_queue_item(state: State<'_, AppState>, item_id: String) -> Result<(), String> {
    state.upload_queue.remove_item(&item_id)
}

#[tauri::command]
pub async fn get_provider_logs(
    provider: String,
    max_lines: Option<usize>,
) -> Result<Vec<LogEntry>, String> {
    read_provider_logs(&provider, max_lines).map_err(|e| e.to_string())
}

// Session sync state for tracking progress
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionSyncProgress {
    pub is_scanning: bool,
    pub is_syncing: bool,
    pub total_sessions: usize,
    pub synced_sessions: usize,
    pub current_provider: String,
    pub current_project: String,
    pub sessions_found: Vec<SessionInfo>,
    pub errors: Vec<String>,
    pub is_complete: bool,
    // Track upload queue state for real progress
    pub initial_queue_size: Option<usize>,
    pub is_uploading: bool,
}

// Provider-specific sync state - using std::sync::OnceLock for thread-safe initialization
use std::sync::OnceLock;
static SYNC_PROGRESS: OnceLock<Arc<Mutex<HashMap<String, SessionSyncProgress>>>> = OnceLock::new();

fn get_sync_progress_map() -> &'static Arc<Mutex<HashMap<String, SessionSyncProgress>>> {
    SYNC_PROGRESS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn get_sync_progress_for_provider(provider_id: &str) -> Result<SessionSyncProgress, String> {
    if let Ok(progress_map) = get_sync_progress_map().lock() {
        Ok(progress_map.get(provider_id).cloned().unwrap_or_default())
    } else {
        Err("Failed to access sync progress".to_string())
    }
}

fn update_sync_progress_for_provider<F>(provider_id: &str, updater: F) -> Result<(), String>
where
    F: FnOnce(&mut SessionSyncProgress),
{
    if let Ok(mut progress_map) = get_sync_progress_map().lock() {
        let progress = progress_map.entry(provider_id.to_string()).or_default();
        updater(progress);
        Ok(())
    } else {
        Err("Failed to access sync progress".to_string())
    }
}

#[tauri::command]
pub async fn scan_historical_sessions(
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<Vec<SessionInfo>, String> {
    use crate::logging::{log_info, log_warn};
    use tauri::Emitter;

    // Log start of scan
    if let Err(e) = log_info(
        &provider_id,
        &format!("🔍 Starting historical session scan for {}", provider_id),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Emit initial progress event
    let _ = app_handle.emit(
        "rescan-progress",
        serde_json::json!({
            "provider": provider_id,
            "phase": "starting",
            "current": 0,
            "total": 0,
            "message": "Starting scan..."
        }),
    );

    // Update progress
    update_sync_progress_for_provider(&provider_id, |progress| {
        progress.is_scanning = true;
        progress.current_provider = provider_id.clone();
        progress.errors.clear();
        progress.sessions_found.clear();
    })
    .ok();

    // Load provider config
    let config = load_provider_config(&provider_id)
        .map_err(|e| format!("Failed to load provider config: {}", e))?;

    if !config.enabled {
        let err_msg = format!("Provider '{}' is not enabled", provider_id);
        if let Err(e) = log_warn(&provider_id, &format!("⚠ {}", err_msg)) {
            eprintln!("Logging error: {}", e);
        }
        return Err(err_msg);
    }

    if let Err(e) = log_info(
        &provider_id,
        &format!("📂 Scanning directory: {}", config.home_directory),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Emit scanning phase
    let _ = app_handle.emit(
        "rescan-progress",
        serde_json::json!({
            "provider": provider_id,
            "phase": "scanning",
            "current": 0,
            "total": 0,
            "message": format!("Scanning directory: {}", config.home_directory)
        }),
    );

    // Prepare project filter (pass to scanner for early filtering)
    let selected_projects_filter = if config.project_selection == "ALL" {
        if let Err(e) = log_info(
            &provider_id,
            "📋 Using ALL project selection - scanning all projects",
        ) {
            eprintln!("Logging error: {}", e);
        }
        None
    } else {
        if let Err(e) = log_info(
            &provider_id,
            &format!(
                "📋 Filtering to {} selected projects: {}",
                config.selected_projects.len(),
                config.selected_projects.join(", ")
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }
        Some(config.selected_projects.as_slice())
    };

    // Scan for sessions with early filtering (avoids scanning/processing unselected projects)
    let sessions = crate::providers::scan_all_sessions_filtered(
        &provider_id,
        &config.home_directory,
        selected_projects_filter,
    )
    .map_err(|e| {
        // Log the error
        if let Err(log_err) = log_warn(&provider_id, &format!("✗ Failed to scan sessions: {}", e))
        {
            eprintln!("Logging error: {}", log_err);
        }
        // Update progress with error
        update_sync_progress_for_provider(&provider_id, |progress| {
            progress.errors.push(e.clone());
            progress.is_scanning = false;
        })
        .ok();
        e
    })?;

    if let Err(e) = log_info(
        &provider_id,
        &format!("✓ Scan complete: found {} sessions", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Emit found sessions count
    let _ = app_handle.emit(
        "rescan-progress",
        serde_json::json!({
            "provider": provider_id,
            "phase": "processing",
            "current": 0,
            "total": sessions.len(),
            "message": format!("Found {} sessions, inserting into database...", sessions.len())
        }),
    );

    // Insert all sessions into the database (just like file watcher does)
    // The upload queue poller will handle uploading them
    let mut inserted_count = 0;
    for (index, session) in sessions.iter().enumerate() {
        // Emit progress every 10 sessions or on last session
        if index % 10 == 0 || index == sessions.len() - 1 {
            let _ = app_handle.emit(
                "rescan-progress",
                serde_json::json!({
                    "provider": provider_id,
                    "phase": "processing",
                    "current": index + 1,
                    "total": sessions.len(),
                    "message": format!("Processing session {} of {}...", index + 1, sessions.len())
                }),
            );
        }
        match crate::providers::common::db_helpers::insert_session_immediately(
            &provider_id,
            &session.project_name,
            &session.session_id,
            &session.file_path,
            session.file_size,
            None, // Hash will be calculated during upload
            true, // is_historical - rescan discovers historical sessions
        ) {
            Ok(_) => {
                inserted_count += 1;
            }
            Err(e) => {
                if let Err(log_err) = log_warn(
                    &provider_id,
                    &format!("⚠ Failed to insert session {}: {}", session.session_id, e),
                ) {
                    eprintln!("Logging error: {}", log_err);
                }
            }
        }
    }

    if let Err(e) = log_info(
        &provider_id,
        &format!("✓ Inserted {} sessions into database", inserted_count),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Emit completion event
    let _ = app_handle.emit(
        "rescan-progress",
        serde_json::json!({
            "provider": provider_id,
            "phase": "complete",
            "current": sessions.len(),
            "total": sessions.len(),
            "message": format!("Scan complete! Found and inserted {} sessions.", inserted_count)
        }),
    );

    // Update progress
    update_sync_progress_for_provider(&provider_id, |progress| {
        progress.is_scanning = false;
        progress.total_sessions = sessions.len();
        progress.sessions_found = sessions.clone();
    })
    .ok();

    Ok(sessions)
}

#[tauri::command]
pub async fn sync_historical_sessions(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    use crate::logging::{log_error, log_info, log_warn};

    if let Err(e) = log_info(
        &provider_id,
        &format!("📤 Starting historical session sync for {}", provider_id),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Load provider config to check sync mode
    let provider_config = load_provider_config(&provider_id)
        .map_err(|e| format!("Failed to load provider config: {}", e))?;

    // Check if sync mode allows uploads
    if provider_config.sync_mode != "Transcript and Metrics" {
        let err_msg = format!(
            "Sync mode is set to '{}'. Please change it to 'Transcript and Metrics' in provider settings to enable synchronization.",
            provider_config.sync_mode
        );
        if let Err(e) = log_warn(&provider_id, &format!("⚠ {}", err_msg)) {
            eprintln!("Logging error: {}", e);
        }
        return Err(err_msg);
    }

    // Update upload queue with current config
    if let Ok(config) = load_config() {
        state.upload_queue.set_config(config);
        if let Err(e) = log_info(&provider_id, "✓ Upload queue configured") {
            eprintln!("Logging error: {}", e);
        }
    } else if let Err(e) = log_warn(&provider_id, "⚠ Failed to load config for upload queue") {
        eprintln!("Logging error: {}", e);
    }

    // Update progress
    update_sync_progress_for_provider(&provider_id, |progress| {
        progress.is_syncing = true;
        progress.synced_sessions = 0;
        progress.is_complete = false;
        progress.errors.clear();
    })
    .ok();

    // Get sessions from progress state (they should have been scanned and filtered first)
    let sessions = get_sync_progress_for_provider(&provider_id)?.sessions_found;

    if sessions.is_empty() {
        let err_msg = "No sessions found to sync. Run scan first.".to_string();
        if let Err(e) = log_warn(&provider_id, &format!("⚠ {}", err_msg)) {
            eprintln!("Logging error: {}", e);
        }
        return Err(err_msg);
    }

    if let Err(e) = log_info(
        &provider_id,
        &format!("📋 Queueing {} sessions for upload", sessions.len()),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Track initial upload queue status to calculate completion
    let _initial_status = state.upload_queue.get_status();

    // Add all sessions to upload queue
    let mut queued_count = 0;
    let mut error_count = 0;
    for (index, session) in sessions.iter().enumerate() {
        // Update current progress
        update_sync_progress_for_provider(&provider_id, |progress| {
            progress.current_project = session.project_name.clone();
        })
        .ok();

        if let Err(e) = log_info(
            &provider_id,
            &format!(
                "  [{}/{}] Queueing session {} (project: {}, cwd: {:?})",
                index + 1,
                sessions.len(),
                session.session_id,
                session.project_name,
                session.cwd
            ),
        ) {
            eprintln!("Logging error: {}", e);
        }

        // Add to upload queue with enhanced metadata
        if let Err(e) = state.upload_queue.add_historical_session(session) {
            let error_msg = format!("Failed to queue {}: {}", session.file_name, e);
            if let Err(log_err) = log_error(&provider_id, &format!("✗ {}", error_msg)) {
                eprintln!("Logging error: {}", log_err);
            }
            update_sync_progress_for_provider(&provider_id, |progress| {
                progress.errors.push(error_msg);
            })
            .ok();
            error_count += 1;
        } else {
            queued_count += 1;
        }
    }

    if let Err(e) = log_info(
        &provider_id,
        &format!(
            "✓ Queued {}/{} sessions ({} errors)",
            queued_count,
            sessions.len(),
            error_count
        ),
    ) {
        eprintln!("Logging error: {}", e);
    }

    // Store initial queue size for progress calculation
    let final_status = state.upload_queue.get_status();
    update_sync_progress_for_provider(&provider_id, |progress| {
        progress.is_syncing = false; // Sessions are queued, now uploads happen in background
        progress.is_uploading = true; // Mark as uploading
        progress.is_complete = false; // Will be determined by polling
        progress.initial_queue_size = Some(final_status.pending);
    })
    .ok();

    if let Err(e) = log_info(
        &provider_id,
        &format!(
            "📊 Upload queue status: {} pending, {} processing",
            final_status.pending, final_status.processing
        ),
    ) {
        eprintln!("Logging error: {}", e);
    }

    Ok(())
}

#[tauri::command]
pub async fn get_session_sync_progress(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<SessionSyncProgress, String> {
    let mut progress = get_sync_progress_for_provider(&provider_id)?;

    // If we're tracking upload progress, calculate real progress from upload queue
    if progress.is_uploading && progress.initial_queue_size.is_some() {
        let current_status = state.upload_queue.get_status();
        let initial_size = progress.initial_queue_size.unwrap();

        // Calculate completed uploads: initial_size - (current pending + processing)
        let currently_in_queue = current_status.pending + current_status.processing;
        let completed = initial_size.saturating_sub(currently_in_queue);

        progress.synced_sessions = completed;

        // Check if all uploads are complete
        if currently_in_queue == 0 && completed > 0 {
            progress.is_uploading = false;
            progress.is_complete = true;
        }
    }

    Ok(progress)
}

#[tauri::command]
pub async fn reset_session_sync_progress(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    // Clear the sync progress state
    if let Ok(mut progress_map) = get_sync_progress_map().lock() {
        progress_map.remove(&provider_id);
    } else {
        return Err("Failed to reset sync progress".to_string());
    }

    // Clear uploaded hashes to allow re-syncing the same files
    state.upload_queue.clear_uploaded_hashes();

    Ok(())
}

#[tauri::command]
pub async fn execute_sql(
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    crate::database::execute_sql_query(&sql, params).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_all_sessions() -> Result<String, String> {
    use crate::logging::log_info;

    // Get counts before deleting
    let metrics_count =
        crate::database::execute_sql_query("SELECT COUNT(*) as count FROM session_metrics", vec![])
            .map_err(|e| e.to_string())?;

    let sessions_count =
        crate::database::execute_sql_query("SELECT COUNT(*) as count FROM agent_sessions", vec![])
            .map_err(|e| e.to_string())?;

    let metrics_num = metrics_count
        .first()
        .and_then(|r| r.get("count"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let sessions_num = sessions_count
        .first()
        .and_then(|r| r.get("count"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // Clear both tables
    crate::database::execute_sql_query("DELETE FROM session_metrics", vec![])
        .map_err(|e| e.to_string())?;

    crate::database::execute_sql_query("DELETE FROM agent_sessions", vec![])
        .map_err(|e| e.to_string())?;

    let message = format!(
        "Cleared {} session metrics and {} sessions from database",
        metrics_num, sessions_num
    );

    let _ = log_info("system", &message);
    println!("{}", message);

    Ok(message)
}

#[tauri::command]
pub async fn clear_provider_sessions(provider_id: String) -> Result<String, String> {
    use crate::logging::log_info;

    // Get counts before deleting
    let metrics_count = crate::database::execute_sql_query(
        "SELECT COUNT(*) as count FROM session_metrics sm
         JOIN agent_sessions s ON sm.session_id = s.session_id
         WHERE s.provider = ?",
        vec![serde_json::Value::String(provider_id.clone())],
    )
    .map_err(|e| e.to_string())?;

    let sessions_count = crate::database::execute_sql_query(
        "SELECT COUNT(*) as count FROM agent_sessions WHERE provider = ?",
        vec![serde_json::Value::String(provider_id.clone())],
    )
    .map_err(|e| e.to_string())?;

    let metrics_num = metrics_count
        .first()
        .and_then(|r| r.get("count"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let sessions_num = sessions_count
        .first()
        .and_then(|r| r.get("count"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // Delete metrics for sessions from this provider
    crate::database::execute_sql_query(
        "DELETE FROM session_metrics WHERE session_id IN
         (SELECT session_id FROM agent_sessions WHERE provider = ?)",
        vec![serde_json::Value::String(provider_id.clone())],
    )
    .map_err(|e| e.to_string())?;

    // Delete sessions from this provider
    crate::database::execute_sql_query(
        "DELETE FROM agent_sessions WHERE provider = ?",
        vec![serde_json::Value::String(provider_id.clone())],
    )
    .map_err(|e| e.to_string())?;

    let message = format!(
        "Cleared {} session metrics and {} sessions for provider '{}'",
        metrics_num, sessions_num, provider_id
    );

    let _ = log_info(&provider_id, &message);
    println!("{}", message);

    Ok(message)
}

#[tauri::command]
pub async fn get_session_content(
    provider: String,
    file_path: String,
    _session_id: String,
) -> Result<String, String> {
    use std::path::PathBuf;
    use tracing::debug;

    let path = PathBuf::from(&file_path);

    // All providers now use cached JSONL files - read directly
    // OpenCode sessions are aggregated to ~/.guidemode/cache/opencode/{session_id}.jsonl
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read session file for {}: {}", provider, e))?;

    debug!(
        provider = %provider,
        path = %path.display(),
        content_length = content.len(),
        "Loaded session content"
    );

    Ok(content)
}

// Autostart function for watchers
pub fn start_enabled_watchers(app_state: &AppState) {
    use tracing::{error, info};

    // Load and set the configuration on upload queue first
    if let Ok(config) = load_config() {
        app_state.upload_queue.set_config(config);
        info!("Configuration loaded and set for upload queue");
    } else {
        error!("Failed to load configuration for upload queue");
    }

    // Try to start Claude Code watcher if enabled
    if let Ok(claude_config) = load_provider_config("claude-code") {
        if claude_config.enabled {
            // Check if home directory exists before starting watcher
            let expanded_path = shellexpand::tilde(&claude_config.home_directory).to_string();
            if !std::path::Path::new(&expanded_path).exists() {
                error!(
                    provider = "claude-code",
                    directory = %claude_config.home_directory,
                    "Cannot start watcher: directory does not exist"
                );
                // Continue without starting this watcher
                // No need to return error - just skip this provider
            } else {
                // Scan for projects
                match crate::providers::scan_projects("claude-code", &claude_config.home_directory)
                {
                    Ok(projects) => {
                        let projects_to_watch = if claude_config.project_selection == "ALL" {
                            projects.iter().map(|p| p.name.clone()).collect()
                        } else {
                            claude_config.selected_projects
                        };

                        if !projects_to_watch.is_empty() {
                            match ClaudeWatcher::new(
                                projects_to_watch,
                                Arc::clone(&app_state.upload_queue),
                                app_state.event_bus.clone(),
                            ) {
                                Ok(watcher) => {
                                    if let Ok(mut watchers) = app_state.watchers.lock() {
                                        watchers.insert(
                                            "claude-code".to_string(),
                                            Watcher::Claude(watcher),
                                        );
                                        info!("Claude Code watcher started automatically");
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "Failed to start Claude Code watcher");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to scan Claude Code projects");
                    }
                }
            }
        }
    }

    // Try to start OpenCode watcher if enabled
    if let Ok(opencode_config) = load_provider_config("opencode") {
        if opencode_config.enabled {
            // Check if home directory exists before starting watcher
            let expanded_path = shellexpand::tilde(&opencode_config.home_directory).to_string();
            if !std::path::Path::new(&expanded_path).exists() {
                error!(
                    provider = "opencode",
                    directory = %opencode_config.home_directory,
                    "Cannot start watcher: directory does not exist"
                );
            } else {
                // Scan for projects
                match crate::providers::scan_projects("opencode", &opencode_config.home_directory) {
                    Ok(projects) => {
                        let projects_to_watch = if opencode_config.project_selection == "ALL" {
                            projects.iter().map(|p| p.name.clone()).collect()
                        } else {
                            opencode_config.selected_projects
                        };

                        if !projects_to_watch.is_empty() {
                            match OpenCodeWatcher::new(
                                projects_to_watch,
                                Arc::clone(&app_state.upload_queue),
                                app_state.event_bus.clone(),
                            ) {
                                Ok(watcher) => {
                                    if let Ok(mut watchers) = app_state.watchers.lock() {
                                        watchers.insert(
                                            "opencode".to_string(),
                                            Watcher::OpenCode(watcher),
                                        );
                                        info!("OpenCode watcher started automatically");
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "Failed to start OpenCode watcher");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to scan OpenCode projects");
                    }
                }
            }
        }
    }

    // Try to start Codex watcher if enabled
    if let Ok(codex_config) = load_provider_config("codex") {
        if codex_config.enabled {
            // Scan for projects
            match crate::providers::scan_projects("codex", &codex_config.home_directory) {
                Ok(projects) => {
                    let projects_to_watch = if codex_config.project_selection == "ALL" {
                        projects.iter().map(|p| p.name.clone()).collect()
                    } else {
                        codex_config.selected_projects
                    };

                    if !projects_to_watch.is_empty() {
                        match CodexWatcher::new(
                            projects_to_watch,
                            Arc::clone(&app_state.upload_queue),
                            app_state.event_bus.clone(),
                        ) {
                            Ok(watcher) => {
                                if let Ok(mut watchers) = app_state.watchers.lock() {
                                    watchers.insert("codex".to_string(), Watcher::Codex(watcher));
                                    info!("Codex watcher started automatically");
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "Failed to start Codex watcher");
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to scan Codex projects");
                }
            }
        }
    }

    // Try to start GitHub Copilot watcher if enabled
    if let Ok(copilot_config) = load_provider_config("github-copilot") {
        if copilot_config.enabled {
            // Scan for projects
            match crate::providers::scan_projects("github-copilot", &copilot_config.home_directory)
            {
                Ok(projects) => {
                    let projects_to_watch = if copilot_config.project_selection == "ALL" {
                        projects.iter().map(|p| p.name.clone()).collect()
                    } else {
                        copilot_config.selected_projects
                    };

                    if !projects_to_watch.is_empty() {
                        match CopilotWatcher::new(
                            projects_to_watch,
                            Arc::clone(&app_state.upload_queue),
                            app_state.event_bus.clone(),
                        ) {
                            Ok(watcher) => {
                                if let Ok(mut watchers) = app_state.watchers.lock() {
                                    watchers.insert(
                                        "github-copilot".to_string(),
                                        Watcher::Copilot(watcher),
                                    );
                                    info!("GitHub Copilot watcher started automatically");
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "Failed to start GitHub Copilot watcher");
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to scan GitHub Copilot projects");
                }
            }
        }
    }

    // Try to start Cursor watcher if enabled
    if let Ok(cursor_config) = load_provider_config("cursor") {
        if cursor_config.enabled {
            // Check if chats directory exists
            let chats_path = shellexpand::tilde("~/.cursor/chats").to_string();
            if !std::path::Path::new(&chats_path).exists() {
                error!(
                    provider = "cursor",
                    directory = %chats_path,
                    "Cannot start watcher: directory does not exist"
                );
            } else {
                // Scan for projects
                match crate::providers::scan_projects("cursor", &cursor_config.home_directory) {
                    Ok(projects) => {
                        let projects_to_watch = if cursor_config.project_selection == "ALL" {
                            projects.iter().map(|p| p.name.clone()).collect()
                        } else {
                            cursor_config.selected_projects
                        };

                        if !projects_to_watch.is_empty() {
                            match CursorWatcher::new(
                                projects_to_watch,
                                Arc::clone(&app_state.upload_queue),
                                app_state.event_bus.clone(),
                            ) {
                                Ok(watcher) => {
                                    if let Ok(mut watchers) = app_state.watchers.lock() {
                                        watchers.insert("cursor".to_string(), Watcher::Cursor(watcher));
                                        info!("Cursor watcher started automatically");
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "Failed to start Cursor watcher");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to scan Cursor projects");
                    }
                }
            }
        }
    }

    // Try to start Gemini Code watcher if enabled
    if let Ok(gemini_config) = load_provider_config("gemini-code") {
        if gemini_config.enabled {
            // Scan for projects - scan_projects now returns hashes in the path field
            match crate::providers::scan_projects("gemini-code", &gemini_config.home_directory) {
                Ok(projects) => {
                    let projects_to_watch = if gemini_config.project_selection == "ALL" {
                        // For ALL mode, pass all hashes (stored in path field)
                        projects.iter().map(|p| p.path.clone()).collect()
                    } else {
                        // selected_projects now contains hashes (not CWDs)
                        gemini_config.selected_projects
                    };

                    if !projects_to_watch.is_empty() {
                        match GeminiWatcher::new(
                            projects_to_watch,
                            Arc::clone(&app_state.upload_queue),
                            app_state.event_bus.clone(),
                        ) {
                            Ok(watcher) => {
                                if let Ok(mut watchers) = app_state.watchers.lock() {
                                    watchers.insert(
                                        "gemini-code".to_string(),
                                        Watcher::Gemini(watcher),
                                    );
                                    info!("Gemini Code watcher started automatically");
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "Failed to start Gemini Code watcher");
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to scan Gemini Code projects");
                }
            }
        }
    }
}

/// Get all projects with session counts
#[tauri::command]
pub async fn get_all_projects() -> Result<Vec<serde_json::Value>, String> {
    use crate::database::get_all_projects;

    let projects = get_all_projects().map_err(|e| format!("Failed to get projects: {}", e))?;

    // Convert to JSON
    let projects_json: Vec<serde_json::Value> = projects
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "name": p.name,
                "githubRepo": p.github_repo,
                "cwd": p.cwd,
                "type": p.project_type,
                "createdAt": p.created_at,
                "updatedAt": p.updated_at,
                "sessionCount": p.session_count,
            })
        })
        .collect();

    Ok(projects_json)
}

/// Get a single project by ID
#[tauri::command]
pub async fn get_project_by_id(project_id: String) -> Result<Option<serde_json::Value>, String> {
    use crate::database::get_project_by_id;

    let project =
        get_project_by_id(&project_id).map_err(|e| format!("Failed to get project: {}", e))?;

    Ok(project.map(|p| {
        serde_json::json!({
            "id": p.id,
            "name": p.name,
            "githubRepo": p.github_repo,
            "cwd": p.cwd,
            "type": p.project_type,
            "createdAt": p.created_at,
            "updatedAt": p.updated_at,
            "sessionCount": p.session_count,
        })
    }))
}

/// Open a folder in the OS file manager (Finder on macOS, Explorer on Windows, etc.)
#[tauri::command]
pub async fn open_folder_in_os(path: String) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}

/// Quick rate a session
#[tauri::command]
pub async fn quick_rate_session(session_id: String, rating: String) -> Result<(), String> {
    use crate::database::quick_rate_session;

    quick_rate_session(&session_id, &rating).map_err(|e| format!("Failed to save rating: {}", e))
}

/// Get assessment rating for a session
#[tauri::command]
pub async fn get_session_rating(session_id: String) -> Result<Option<String>, String> {
    use crate::database::get_session_rating;

    get_session_rating(&session_id).map_err(|e| format!("Failed to get rating: {}", e))
}

/// Get git diff between two commits for a session with timestamp filtering
#[tauri::command]
pub async fn get_session_git_diff(
    cwd: String,
    first_commit_hash: String,
    latest_commit_hash: String,
    is_active: bool,
    session_start_time: Option<i64>,
    session_end_time: Option<i64>,
) -> Result<Vec<crate::git_diff::FileDiff>, String> {
    crate::git_diff::get_commit_diff(
        &cwd,
        &first_commit_hash,
        &latest_commit_hash,
        is_active,
        session_start_time,
        session_end_time,
    )
}

/// Scan a directory for context files (CLAUDE.md, AGENTS.md, GEMINI.md)
/// Respects .gitignore patterns
#[tauri::command]
pub async fn scan_context_files(
    cwd: String,
) -> Result<Vec<crate::context_files::ContextFile>, String> {
    crate::context_files::scan_context_files(&cwd)
}

/// Scan .claude directory for commands, skills, and config files
#[tauri::command]
pub async fn scan_claude_files(
    cwd: String,
) -> Result<Vec<crate::claude_files::ClaudeFile>, String> {
    crate::claude_files::scan_claude_files(&cwd)
}

/// Log updater events from the frontend
#[tauri::command]
pub async fn log_updater_event_command(
    level: String,
    message: String,
    details: Option<serde_json::Value>,
) -> Result<(), String> {
    crate::logging::log_updater_event(&level, &message, details).map_err(|e| e.to_string())
}

/// Migration report for canonical format migration
#[derive(Debug, Serialize, Deserialize)]
pub struct MigrationReport {
    pub provider: String,
    pub total_sessions: usize,
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    pub dry_run: bool,
}

/// Migrate provider sessions to canonical format
///
/// This command converts existing provider sessions to the unified canonical JSONL format.
/// Supports dry-run mode for validation before actual migration.
///
/// # Arguments
/// * `provider` - Provider name ("codex", "gemini-code", "github-copilot", "opencode")
/// * `dry_run` - If true, validates conversion without writing files
#[tauri::command]
pub async fn migrate_to_canonical_command(
    provider: String,
    dry_run: bool,
) -> Result<MigrationReport, String> {
    use tracing::info;

    info!(
        provider = %provider,
        dry_run = dry_run,
        "Starting migration to canonical format"
    );

    match provider.as_str() {
        "codex" => migrate_codex(dry_run).await,
        "gemini-code" | "github-copilot" | "opencode" => {
            Err(format!("Provider '{}' migration not yet implemented. Coming in Phase 1.", provider))
        }
        other => Err(format!("Unsupported provider: {}", other)),
    }
}

async fn migrate_codex(dry_run: bool) -> Result<MigrationReport, String> {
    use std::path::PathBuf;
    use tracing::{error, info};

    let home = std::env::var("HOME").map_err(|_| "HOME environment variable not set")?;
    let sessions_dir = PathBuf::from(&home).join(".codex/sessions");
    let cache_dir = PathBuf::from(&home).join(".guidemode/cache/canonical/codex");

    if !sessions_dir.exists() {
        return Err(format!(
            "Codex sessions directory not found: {}",
            sessions_dir.display()
        ));
    }

    // Create cache directory if not in dry-run mode
    if !dry_run {
        fs::create_dir_all(&cache_dir).map_err(|e| {
            format!(
                "Failed to create canonical cache directory: {}",
                e
            )
        })?;
        info!(path = %cache_dir.display(), "Created canonical cache directory");
    }

    let mut report = MigrationReport {
        provider: "codex".to_string(),
        total_sessions: 0,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
        dry_run,
    };

    // Find all Codex JSONL files recursively
    let walker = walkdir::WalkDir::new(&sessions_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                == Some("jsonl")
        });

    for entry in walker {
        let path = entry.path();
        report.total_sessions += 1;

        match convert_codex_session(path, &cache_dir, dry_run) {
            Ok(session_id) => {
                report.successful += 1;
                info!(
                    session_id = %session_id,
                    path = %path.display(),
                    "Successfully converted session"
                );
            }
            Err(e) => {
                report.failed += 1;
                let error_msg = format!("{}: {}", path.display(), e);
                report.errors.push(error_msg.clone());
                error!(error = %e, path = %path.display(), "Failed to convert session");
            }
        }
    }

    info!(
        total = report.total_sessions,
        successful = report.successful,
        failed = report.failed,
        dry_run = report.dry_run,
        "Migration completed"
    );

    Ok(report)
}

fn convert_codex_session(
    source: &std::path::Path,
    cache_dir: &std::path::Path,
    dry_run: bool,
) -> Result<String, String> {
    use crate::providers::canonical::converter::ToCanonical;
    use crate::providers::codex::CodexMessage;

    // Read source file
    let content = fs::read_to_string(source)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    if content.trim().is_empty() {
        return Err("Empty session file".to_string());
    }

    // Parse and convert each line
    let mut canonical_lines = Vec::new();
    let mut session_id = String::new();

    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse Codex message
        let codex_msg: CodexMessage = serde_json::from_str(line).map_err(|e| {
            format!("Failed to parse line {}: {}", line_num + 1, e)
        })?;

        // Extract session ID from first session_meta message
        if session_id.is_empty() {
            if let Some(id) = codex_msg.get_session_id() {
                session_id = id;
            }
        }

        // Convert to canonical format
        let canonical = codex_msg
            .to_canonical()
            .map_err(|e| format!("Failed to convert line {}: {}", line_num + 1, e))?;

        // Serialize to JSON
        let json = serde_json::to_string(&canonical)
            .map_err(|e| format!("Failed to serialize line {}: {}", line_num + 1, e))?;

        canonical_lines.push(json);
    }

    // Use session ID from content, or derive from filename
    if session_id.is_empty() {
        session_id = source
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid filename")?
            .to_string();
    }

    // Write canonical JSONL if not dry-run
    if !dry_run {
        let target = cache_dir.join(format!("{}.jsonl", session_id));
        let canonical_content = canonical_lines.join("\n");

        fs::write(&target, canonical_content)
            .map_err(|e| format!("Failed to write canonical file: {}", e))?;
    }

    Ok(session_id)
}
