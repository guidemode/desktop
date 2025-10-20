use std::collections::HashMap;
use std::time::Instant;

/// Tracks state for a single session across file changes
#[derive(Debug, Clone)]
pub struct SessionState {
    pub last_modified: Instant,
    pub last_size: u64,
    pub is_active: bool,
    pub last_seen_time: Option<Instant>,
}

impl SessionState {
    /// Create a new session state with the initial file size
    pub fn new(file_size: u64) -> Self {
        Self {
            last_modified: Instant::now(),
            last_size: file_size,
            is_active: true,
            last_seen_time: None,
        }
    }

    /// Update state with new file change event
    pub fn update(&mut self, file_size: u64) {
        self.last_modified = Instant::now();
        self.last_size = file_size;
        self.is_active = true;
    }

    /// Mark this session as seen (for "new" vs "changed" logging)
    pub fn mark_as_seen(&mut self) {
        self.last_seen_time = Some(Instant::now());
    }

    /// Should we log this change?
    /// Returns true if this is a new session or if the size change is significant
    pub fn should_log(&self, new_size: u64, min_size_change: u64, is_new: bool) -> bool {
        is_new || new_size.saturating_sub(self.last_size) >= min_size_change
    }
}

/// Manager for tracking multiple session states
pub struct SessionStateManager {
    states: HashMap<String, SessionState>,
}

impl SessionStateManager {
    /// Create a new session state manager
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// Get an existing session state or create a new one
    pub fn get_or_create(&mut self, session_id: &str, file_size: u64) -> &mut SessionState {
        self.states
            .entry(session_id.to_string())
            .or_insert_with(|| SessionState::new(file_size))
    }

    /// Check if a session exists in the manager
    pub fn contains(&self, session_id: &str) -> bool {
        self.states.contains_key(session_id)
    }
}

impl Default for SessionStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_new() {
        let state = SessionState::new(1024);
        assert_eq!(state.last_size, 1024);
        assert!(state.is_active);
        assert!(state.last_seen_time.is_none());
    }

    #[test]
    fn test_session_state_update() {
        let mut state = SessionState::new(1024);

        state.update(2048);
        assert_eq!(state.last_size, 2048);
        assert!(state.is_active);
    }

    #[test]
    fn test_mark_as_seen() {
        let mut state = SessionState::new(1024);
        assert!(state.last_seen_time.is_none());

        state.mark_as_seen();
        assert!(state.last_seen_time.is_some());
    }

    #[test]
    fn test_should_log() {
        let state = SessionState::new(1024);
        let min_size_change = 512;

        // New session should always log
        assert!(state.should_log(1024, min_size_change, true));

        // Significant size change should log
        assert!(state.should_log(2048, min_size_change, false));

        // Small size change should not log
        assert!(!state.should_log(1100, min_size_change, false));
    }

    #[test]
    fn test_session_state_manager() {
        let mut manager = SessionStateManager::new();

        // Create a new session
        let state1 = manager.get_or_create("session1", 1024);
        assert_eq!(state1.last_size, 1024);

        // Get existing session
        let state2 = manager.get_or_create("session1", 2048);
        assert_eq!(state2.last_size, 1024); // Should not change existing

        // Check contains
        assert!(manager.contains("session1"));
        assert!(!manager.contains("session2"));
    }
}
