//! Retry logic with exponential backoff and error classification.
//!
//! Handles retry strategy, error classification (client/server/network),
//! and backoff calculation. Extracted from processor.rs in Phase 5.

use chrono::Utc;
use super::super::types::UploadItem;

/// Error classification for determining retry behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    /// Client errors (400-499) - invalid input, don't retry
    ClientError,
    /// Server errors (500-599) - temporary issues, retry with backoff
    ServerError,
    /// Network errors - connection issues, retry with backoff
    NetworkError,
}

/// Retry strategy configuration
pub struct RetryStrategy {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay in seconds for exponential backoff
    pub base_delay_seconds: u64,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_seconds: 2,
        }
    }
}

impl RetryStrategy {
    /// Create a custom retry strategy
    #[cfg(test)]
    pub fn new(max_retries: u32, base_delay_seconds: u64) -> Self {
        Self {
            max_retries,
            base_delay_seconds,
        }
    }

    /// Check if we should retry an item based on current retry count
    pub fn should_retry(&self, item: &UploadItem, error_type: ErrorType) -> bool {
        // Never retry client errors
        if error_type == ErrorType::ClientError {
            return false;
        }

        // Retry server/network errors up to max_retries
        item.retry_count < self.max_retries
    }

    /// Calculate exponential backoff delay
    pub fn calculate_backoff(&self, retry_count: u32) -> u64 {
        // Exponential backoff: base_delay * 2^retry_count
        // Cap at reasonable maximum (256 seconds for default base=2)
        self.base_delay_seconds.saturating_pow(retry_count + 1)
    }

    /// Schedule next retry time for an item
    pub fn schedule_retry(&self, item: &mut UploadItem) {
        let delay_seconds = self.calculate_backoff(item.retry_count);
        item.next_retry_at = Some(Utc::now() + chrono::Duration::seconds(delay_seconds as i64));
    }
}

/// Classify an error message into an ErrorType
pub fn classify_error(error: &str) -> ErrorType {
    // Check for client errors (4xx)
    if error.contains("status 400")
        || error.contains("Bad Request")
        || error.contains("status 401")
        || error.contains("Unauthorized")
        || error.contains("status 403")
        || error.contains("Forbidden")
        || error.contains("status 404")
        || error.contains("Not Found")
        || error.contains("validation failed")
        || error.contains("invalid input")
    {
        return ErrorType::ClientError;
    }

    // Check for server errors (5xx)
    if error.contains("status 5")
        || error.contains("Internal Server Error")
        || error.contains("Service Unavailable")
        || error.contains("Gateway Timeout")
    {
        return ErrorType::ServerError;
    }

    // Default to network error (connection issues, timeouts, etc.)
    ErrorType::NetworkError
}

/// Helper function to check if we should retry (uses default strategy)
pub fn should_retry(item: &UploadItem, error_type: ErrorType) -> bool {
    RetryStrategy::default().should_retry(item, error_type)
}

/// Helper function to schedule retry (uses default strategy)
pub fn schedule_retry(item: &mut UploadItem) {
    RetryStrategy::default().schedule_retry(item);
}

/// Helper function to calculate backoff delay (uses default strategy)
pub fn calculate_backoff(retry_count: u32) -> u64 {
    RetryStrategy::default().calculate_backoff(retry_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_client_errors() {
        assert_eq!(classify_error("status 400"), ErrorType::ClientError);
        assert_eq!(classify_error("Bad Request"), ErrorType::ClientError);
        assert_eq!(classify_error("status 401"), ErrorType::ClientError);
        assert_eq!(classify_error("Unauthorized"), ErrorType::ClientError);
        assert_eq!(classify_error("status 403"), ErrorType::ClientError);
        assert_eq!(classify_error("Forbidden"), ErrorType::ClientError);
        assert_eq!(classify_error("status 404"), ErrorType::ClientError);
        assert_eq!(classify_error("Not Found"), ErrorType::ClientError);
        assert_eq!(classify_error("validation failed"), ErrorType::ClientError);
        assert_eq!(classify_error("invalid input"), ErrorType::ClientError);
    }

    #[test]
    fn test_classify_server_errors() {
        assert_eq!(classify_error("status 500"), ErrorType::ServerError);
        assert_eq!(classify_error("status 502"), ErrorType::ServerError);
        assert_eq!(classify_error("status 503"), ErrorType::ServerError);
        assert_eq!(
            classify_error("Internal Server Error"),
            ErrorType::ServerError
        );
        assert_eq!(
            classify_error("Service Unavailable"),
            ErrorType::ServerError
        );
        assert_eq!(classify_error("Gateway Timeout"), ErrorType::ServerError);
    }

    #[test]
    fn test_classify_network_errors() {
        assert_eq!(
            classify_error("Connection refused"),
            ErrorType::NetworkError
        );
        assert_eq!(classify_error("Timeout"), ErrorType::NetworkError);
        assert_eq!(
            classify_error("DNS resolution failed"),
            ErrorType::NetworkError
        );
        assert_eq!(
            classify_error("Unknown error"),
            ErrorType::NetworkError
        );
    }

    #[test]
    fn test_default_retry_strategy() {
        let strategy = RetryStrategy::default();
        assert_eq!(strategy.max_retries, 3);
        assert_eq!(strategy.base_delay_seconds, 2);
    }

    #[test]
    fn test_custom_retry_strategy() {
        let strategy = RetryStrategy::new(5, 3);
        assert_eq!(strategy.max_retries, 5);
        assert_eq!(strategy.base_delay_seconds, 3);
    }

    #[test]
    fn test_should_retry_client_error() {
        let strategy = RetryStrategy::default();
        let mut item = create_test_item();

        // Client errors should never retry
        item.retry_count = 0;
        assert!(!strategy.should_retry(&item, ErrorType::ClientError));

        item.retry_count = 1;
        assert!(!strategy.should_retry(&item, ErrorType::ClientError));

        item.retry_count = 5;
        assert!(!strategy.should_retry(&item, ErrorType::ClientError));
    }

    #[test]
    fn test_should_retry_server_error() {
        let strategy = RetryStrategy::default();
        let mut item = create_test_item();

        // Server errors should retry up to max_retries
        item.retry_count = 0;
        assert!(strategy.should_retry(&item, ErrorType::ServerError));

        item.retry_count = 1;
        assert!(strategy.should_retry(&item, ErrorType::ServerError));

        item.retry_count = 2;
        assert!(strategy.should_retry(&item, ErrorType::ServerError));

        item.retry_count = 3;
        assert!(!strategy.should_retry(&item, ErrorType::ServerError));

        item.retry_count = 4;
        assert!(!strategy.should_retry(&item, ErrorType::ServerError));
    }

    #[test]
    fn test_should_retry_network_error() {
        let strategy = RetryStrategy::default();
        let mut item = create_test_item();

        // Network errors should retry up to max_retries
        item.retry_count = 0;
        assert!(strategy.should_retry(&item, ErrorType::NetworkError));

        item.retry_count = 1;
        assert!(strategy.should_retry(&item, ErrorType::NetworkError));

        item.retry_count = 2;
        assert!(strategy.should_retry(&item, ErrorType::NetworkError));

        item.retry_count = 3;
        assert!(!strategy.should_retry(&item, ErrorType::NetworkError));
    }

    #[test]
    fn test_calculate_backoff() {
        let strategy = RetryStrategy::default();

        // base = 2, so backoff = 2^(retry_count + 1)
        assert_eq!(strategy.calculate_backoff(0), 2); // 2^1 = 2
        assert_eq!(strategy.calculate_backoff(1), 4); // 2^2 = 4
        assert_eq!(strategy.calculate_backoff(2), 8); // 2^3 = 8
        assert_eq!(strategy.calculate_backoff(3), 16); // 2^4 = 16
    }

    #[test]
    fn test_calculate_backoff_custom_base() {
        let strategy = RetryStrategy::new(3, 3);

        // base = 3, so backoff = 3^(retry_count + 1)
        assert_eq!(strategy.calculate_backoff(0), 3); // 3^1 = 3
        assert_eq!(strategy.calculate_backoff(1), 9); // 3^2 = 9
        assert_eq!(strategy.calculate_backoff(2), 27); // 3^3 = 27
    }

    #[test]
    fn test_schedule_retry() {
        let strategy = RetryStrategy::default();
        let mut item = create_test_item();

        // Before scheduling
        assert!(item.next_retry_at.is_none());

        // Schedule retry for first attempt
        item.retry_count = 0;
        strategy.schedule_retry(&mut item);

        // Should have next_retry_at set
        assert!(item.next_retry_at.is_some());

        // Should be approximately 2 seconds in the future
        let next_retry = item.next_retry_at.unwrap();
        let now = Utc::now();
        let diff = (next_retry - now).num_seconds();
        assert!(diff >= 1 && diff <= 3, "Expected ~2s, got {}s", diff);
    }

    #[test]
    fn test_schedule_retry_incremental() {
        let strategy = RetryStrategy::default();
        let mut item = create_test_item();

        // Test exponential increase
        item.retry_count = 0;
        strategy.schedule_retry(&mut item);
        let first_retry = item.next_retry_at.unwrap();

        item.retry_count = 1;
        strategy.schedule_retry(&mut item);
        let second_retry = item.next_retry_at.unwrap();

        // Second retry should be later than first
        assert!(second_retry > first_retry);
    }

    #[test]
    fn test_helper_should_retry() {
        let mut item = create_test_item();

        // Test helper function
        item.retry_count = 0;
        assert!(should_retry(&item, ErrorType::ServerError));

        item.retry_count = 3;
        assert!(!should_retry(&item, ErrorType::ServerError));

        assert!(!should_retry(&item, ErrorType::ClientError));
    }

    #[test]
    fn test_helper_calculate_backoff() {
        // Test helper function
        assert_eq!(calculate_backoff(0), 2);
        assert_eq!(calculate_backoff(1), 4);
        assert_eq!(calculate_backoff(2), 8);
    }

    #[test]
    fn test_helper_schedule_retry() {
        let mut item = create_test_item();

        // Test helper function
        assert!(item.next_retry_at.is_none());
        schedule_retry(&mut item);
        assert!(item.next_retry_at.is_some());
    }

    // Helper to create test item
    fn create_test_item() -> UploadItem {
        use std::path::PathBuf;

        UploadItem {
            id: "test-id".to_string(),
            provider: "test-provider".to_string(),
            project_name: "test-project".to_string(),
            file_path: PathBuf::from("/test/path"),
            file_name: "test.jsonl".to_string(),
            queued_at: Utc::now(),
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            file_hash: None,
            file_size: 1024,
            session_id: Some("test-session".to_string()),
            content: None,
            cwd: None,
        }
    }
}
