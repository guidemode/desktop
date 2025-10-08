use thiserror::Error;

/// GuideAI Desktop application errors
#[derive(Debug, Error)]
pub enum GuideAIError {
    /// Database-related errors
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Configuration file errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Upload/sync errors
    #[error("Upload error: {0}")]
    Upload(String),

    /// Authentication errors
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Validation errors (path, size, etc.)
    #[error("Validation error: {0}")]
    Validation(String),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP request errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Mutex poison error
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),

    /// Generic error with context
    #[error("{0}")]
    Other(String),
}

/// Convert GuideAIError to String for Tauri commands
/// Tauri commands can only return String errors to the frontend
impl From<GuideAIError> for String {
    fn from(err: GuideAIError) -> String {
        err.to_string()
    }
}

/// Helper to convert Box<dyn std::error::Error> to GuideAIError
impl From<Box<dyn std::error::Error>> for GuideAIError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        GuideAIError::Other(err.to_string())
    }
}

/// Helper trait for adding context to errors
#[allow(dead_code)]
pub trait ErrorContext<T> {
    fn context(self, msg: &str) -> Result<T, GuideAIError>;
}

impl<T, E: Into<GuideAIError>> ErrorContext<T> for Result<T, E> {
    fn context(self, msg: &str) -> Result<T, GuideAIError> {
        self.map_err(|e| {
            let err: GuideAIError = e.into();
            match err {
                GuideAIError::Other(s) => GuideAIError::Other(format!("{}: {}", msg, s)),
                GuideAIError::Database(e) => GuideAIError::Database(e),
                GuideAIError::Config(s) => GuideAIError::Config(format!("{}: {}", msg, s)),
                GuideAIError::Upload(s) => GuideAIError::Upload(format!("{}: {}", msg, s)),
                GuideAIError::Auth(s) => GuideAIError::Auth(format!("{}: {}", msg, s)),
                GuideAIError::Validation(s) => {
                    GuideAIError::Validation(format!("{}: {}", msg, s))
                }
                GuideAIError::Io(e) => GuideAIError::Io(e),
                GuideAIError::Json(e) => GuideAIError::Json(e),
                GuideAIError::Http(e) => GuideAIError::Http(e),
                GuideAIError::LockPoisoned(s) => {
                    GuideAIError::LockPoisoned(format!("{}: {}", msg, s))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = GuideAIError::Validation("Invalid path".to_string());
        assert_eq!(err.to_string(), "Validation error: Invalid path");
    }

    #[test]
    fn test_error_conversion_to_string() {
        let err = GuideAIError::Config("Missing API key".to_string());
        let s: String = err.into();
        assert_eq!(s, "Configuration error: Missing API key");
    }

    #[test]
    fn test_error_context() {
        let result: Result<(), std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));
        let result = result.context("Failed to read config file");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("I/O error"));
    }
}
