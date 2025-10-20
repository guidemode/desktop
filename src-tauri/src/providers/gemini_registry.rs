use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Registry entry for a Gemini project
/// Maps project hash to its working directory and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiProjectEntry {
    /// The working directory path (e.g., "/Users/cliftonc/work/guideai")
    pub cwd: String,

    /// Human-readable project name (extracted from CWD, e.g., "guideai")
    pub name: String,

    /// Last time this project was seen (ISO 8601 timestamp)
    #[serde(rename = "lastSeen")]
    pub last_seen: String,
}

/// Registry that maps Gemini project hashes to their metadata
/// Stored at ~/.guideai/providers/gemini-code-projects.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeminiProjectRegistry {
    /// Map of hash -> project entry
    pub projects: HashMap<String, GeminiProjectEntry>,
}

impl GeminiProjectRegistry {
    /// Load the registry from disk
    /// Returns empty registry if file doesn't exist
    pub fn load() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = Self::get_registry_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let registry: GeminiProjectRegistry = serde_json::from_str(&content)?;
        Ok(registry)
    }

    /// Save the registry to disk
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = Self::get_registry_path()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;

        // Set permissions to 600 (read/write for owner only) on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&path, permissions)?;
        }

        Ok(())
    }

    /// Update or insert a project entry
    pub fn update_project(
        &mut self,
        hash: String,
        cwd: String,
        name: String,
    ) {
        let now = chrono::Utc::now().to_rfc3339();

        self.projects.insert(
            hash,
            GeminiProjectEntry {
                cwd,
                name,
                last_seen: now,
            },
        );
    }

    /// Get a project entry by hash
    pub fn get_project(&self, hash: &str) -> Option<&GeminiProjectEntry> {
        self.projects.get(hash)
    }

    /// Get the path to the registry file
    fn get_registry_path() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let config_dir = crate::config::get_providers_dir()
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::other(e.to_string()))
            })?;
        Ok(config_dir.join("gemini-code-projects.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new_empty() {
        let registry = GeminiProjectRegistry::default();
        assert_eq!(registry.projects.len(), 0);
    }

    #[test]
    fn test_registry_update_project() {
        let mut registry = GeminiProjectRegistry::default();

        registry.update_project(
            "abc123".to_string(),
            "/Users/test/project".to_string(),
            "project".to_string(),
        );

        assert_eq!(registry.projects.len(), 1);
        let entry = registry.get_project("abc123").unwrap();
        assert_eq!(entry.cwd, "/Users/test/project");
        assert_eq!(entry.name, "project");
    }

    #[test]
    fn test_registry_get_project() {
        let mut registry = GeminiProjectRegistry::default();

        registry.update_project(
            "hash1".to_string(),
            "/path1".to_string(),
            "proj1".to_string(),
        );

        assert!(registry.get_project("hash1").is_some());
        assert!(registry.get_project("nonexistent").is_none());
    }

    #[test]
    fn test_registry_serialize_deserialize() {
        let mut registry = GeminiProjectRegistry::default();
        registry.update_project(
            "test-hash".to_string(),
            "/test/path".to_string(),
            "testproj".to_string(),
        );

        let json = serde_json::to_string(&registry).unwrap();
        let deserialized: GeminiProjectRegistry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.projects.len(), 1);
        let entry = deserialized.get_project("test-hash").unwrap();
        assert_eq!(entry.cwd, "/test/path");
        assert_eq!(entry.name, "testproj");
    }
}
