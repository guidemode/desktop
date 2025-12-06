use serde::{Deserialize, Serialize};
use std::fmt;

/// Session ID newtype for type safety
/// Prevents accidentally passing project_id where session_id is expected
///
/// Note: This is a Phase 4 improvement for gradual adoption across the codebase.
/// Currently unused but available for type-safe refactoring.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    #[allow(dead_code)]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Repository ID newtype for type safety
///
/// Note: This is a Phase 4 improvement for gradual adoption across the codebase.
/// Currently unused but available for type-safe refactoring.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryId(String);

impl RepositoryId {
    #[allow(dead_code)]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RepositoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RepositoryId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RepositoryId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for RepositoryId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_creation() {
        let id = SessionId::new("session-123");
        assert_eq!(id.as_str(), "session-123");
        assert_eq!(id.to_string(), "session-123");
    }

    #[test]
    fn test_session_id_from_string() {
        let id: SessionId = "session-456".into();
        assert_eq!(id.as_str(), "session-456");
    }

    #[test]
    fn test_repository_id_creation() {
        let id = RepositoryId::new("repository-789");
        assert_eq!(id.as_str(), "repository-789");
        assert_eq!(id.to_string(), "repository-789");
    }

    #[test]
    fn test_repository_id_from_string() {
        let id: RepositoryId = String::from("repository-abc").into();
        assert_eq!(id.as_str(), "repository-abc");
    }

    #[test]
    fn test_session_id_equality() {
        let id1 = SessionId::new("session-1");
        let id2 = SessionId::new("session-1");
        let id3 = SessionId::new("session-2");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }
}
