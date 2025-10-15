use std::time::Duration;

// Size thresholds (used for logging decisions only)
pub const MIN_SIZE_CHANGE_BYTES: u64 = 1024; // 1KB minimum change to log

// Polling intervals
pub const FILE_WATCH_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub const EVENT_TIMEOUT: Duration = Duration::from_secs(5);
