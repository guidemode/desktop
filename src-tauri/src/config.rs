use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuideModeConfig {
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(rename = "serverUrl")]
    pub server_url: Option<String>,
    pub username: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: Option<String>,
    #[serde(rename = "tenantId")]
    pub tenant_id: Option<String>,
    #[serde(rename = "tenantName")]
    pub tenant_name: Option<String>,
}

pub fn get_config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(home_dir) = dirs::home_dir() {
        Ok(home_dir.join(".guidemode"))
    } else {
        Err("Could not find home directory".into())
    }
}

pub fn get_config_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(get_config_dir()?.join("config.json"))
}

pub fn ensure_config_dir() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = get_config_dir()?;
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;

        // Set permissions to 700 (read/write/execute for owner only) on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&config_dir)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&config_dir, permissions)?;
        }
    }
    Ok(())
}

pub fn load_config() -> Result<GuideModeConfig, Box<dyn std::error::Error>> {
    ensure_config_dir()?;

    let config_file = get_config_file_path()?;

    if config_file.exists() {
        let content = fs::read_to_string(config_file)?;
        let config: GuideModeConfig = serde_json::from_str(&content)?;
        Ok(config)
    } else {
        Ok(GuideModeConfig::default())
    }
}

pub fn save_config(config: &GuideModeConfig) -> Result<(), Box<dyn std::error::Error>> {
    ensure_config_dir()?;

    let config_file = get_config_file_path()?;
    let content = serde_json::to_string_pretty(config)?;

    fs::write(&config_file, content)?;

    // Set permissions to 600 (read/write for owner only) on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&config_file)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&config_file, permissions)?;
    }

    Ok(())
}

pub fn clear_config() -> Result<(), Box<dyn std::error::Error>> {
    let default_config = GuideModeConfig::default();
    save_config(&default_config)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub enabled: bool,
    #[serde(rename = "homeDirectory")]
    pub home_directory: String,
    #[serde(rename = "projectSelection")]
    pub project_selection: String, // "ALL" or "SELECTED"
    #[serde(rename = "selectedProjects")]
    pub selected_projects: Vec<String>,
    #[serde(rename = "lastScanned")]
    pub last_scanned: Option<String>,
    #[serde(rename = "syncMode", default = "default_sync_mode")]
    pub sync_mode: String, // "Nothing", "Metrics Only", or "Transcript and Metrics"
}

fn default_sync_mode() -> String {
    "Nothing".to_string()
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            home_directory: String::new(),
            project_selection: "ALL".to_string(),
            selected_projects: Vec::new(),
            last_scanned: None,
            sync_mode: "Nothing".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    #[serde(rename = "lastModified")]
    pub last_modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub log_type: String,
    pub provider: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

pub fn get_providers_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(get_config_dir()?.join("providers"))
}

pub fn get_logs_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(get_config_dir()?.join("logs"))
}

pub fn ensure_providers_dir() -> Result<(), Box<dyn std::error::Error>> {
    let providers_dir = get_providers_dir()?;
    if !providers_dir.exists() {
        fs::create_dir_all(&providers_dir)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&providers_dir)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&providers_dir, permissions)?;
        }
    }
    Ok(())
}

pub fn ensure_logs_dir() -> Result<(), Box<dyn std::error::Error>> {
    let logs_dir = get_logs_dir()?;
    if !logs_dir.exists() {
        fs::create_dir_all(&logs_dir)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&logs_dir)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&logs_dir, permissions)?;
        }
    }
    Ok(())
}

pub fn get_provider_config_path(provider_id: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(get_providers_dir()?.join(format!("{}.json", provider_id)))
}

pub fn load_provider_config(
    provider_id: &str,
) -> Result<ProviderConfig, Box<dyn std::error::Error>> {
    ensure_providers_dir()?;

    let config_file = get_provider_config_path(provider_id)?;

    if config_file.exists() {
        let content = fs::read_to_string(config_file)?;
        let config: ProviderConfig = serde_json::from_str(&content)?;
        Ok(config)
    } else {
        Ok(ProviderConfig::default())
    }
}

pub fn save_provider_config(
    provider_id: &str,
    config: &ProviderConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_providers_dir()?;

    let config_file = get_provider_config_path(provider_id)?;
    let content = serde_json::to_string_pretty(config)?;

    fs::write(&config_file, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&config_file)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&config_file, permissions)?;
    }

    Ok(())
}

pub fn delete_provider_config(provider_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config_file = get_provider_config_path(provider_id)?;

    if config_file.exists() {
        fs::remove_file(config_file)?;
    }

    Ok(())
}

/// Determines whether a project should be included based on the provider configuration.
///
/// Returns `true` if:
/// - `project_selection` is "ALL", OR
/// - `project_selection` is "SELECTED" and the project is in `selected_projects`
///
/// Returns `false` otherwise.
#[allow(dead_code)] // Helper function, may be used in future
pub fn should_include_project(project: &str, config: &ProviderConfig) -> bool {
    if config.project_selection == "ALL" {
        return true;
    }

    config.selected_projects.contains(&project.to_string())
}
