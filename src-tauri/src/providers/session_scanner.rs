//! Session scanner - delegates to provider-specific scanners
//!
//! This module provides a unified interface for scanning sessions across all providers.
//! Each provider has its own scanner module that handles provider-specific logic.

use crate::providers::common::SessionInfo;
use shellexpand::tilde;
use std::path::Path;

#[allow(dead_code)] // Will be removed during provider file reorganization
pub fn scan_all_sessions(
    provider_id: &str,
    home_directory: &str,
) -> Result<Vec<SessionInfo>, String> {
    scan_all_sessions_filtered(provider_id, home_directory, None)
}

pub fn scan_all_sessions_filtered(
    provider_id: &str,
    home_directory: &str,
    selected_projects: Option<&[String]>,
) -> Result<Vec<SessionInfo>, String> {
    let expanded = tilde(home_directory);
    let base_path = Path::new(expanded.as_ref());

    if !base_path.exists() {
        return Ok(Vec::new());
    }

    match provider_id {
        "claude-code" => super::claude::scanner::scan_sessions_filtered(base_path, selected_projects),
        "github-copilot" => super::copilot::scanner::scan_sessions_filtered(base_path, selected_projects),
        "opencode" => super::opencode::scanner::scan_sessions_filtered(base_path, selected_projects),
        "codex" => super::codex::scanner::scan_sessions_filtered(base_path, selected_projects),
        "gemini-code" => super::gemini::scanner::scan_sessions_filtered(base_path, selected_projects),
        "cursor" => super::cursor::scanner::scan_sessions_filtered(selected_projects),
        _ => Err(format!("Unsupported provider: {}", provider_id)),
    }
}
