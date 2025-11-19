use tokio::sync::broadcast;

/// Coordinates graceful shutdown across event handlers and watchers
///
/// Usage:
/// ```no_run
/// use guidemode_desktop::shutdown::ShutdownCoordinator;
/// use tokio::sync::mpsc;
///
/// # async fn example() {
/// let coordinator = ShutdownCoordinator::new();
///
/// // In event handlers/watchers:
/// let mut shutdown_rx = coordinator.subscribe();
/// let (tx, mut event_rx) = mpsc::channel::<String>(10);
/// loop {
///     tokio::select! {
///         event = event_rx.recv() => { /* handle event */ }
///         _ = shutdown_rx.recv() => {
///             // Graceful shutdown
///             break;
///         }
///     }
/// }
///
/// // To trigger shutdown:
/// coordinator.shutdown();
/// # }
/// ```
pub struct ShutdownCoordinator {
    shutdown_tx: broadcast::Sender<()>,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(10);
        Self { shutdown_tx }
    }

    /// Subscribe to shutdown signals
    /// Returns a receiver that will receive a message when shutdown is initiated
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Trigger graceful shutdown
    /// All subscribers will receive a shutdown signal
    ///
    /// Note: Currently not exposed as a command, but available for future use
    /// (e.g., graceful app shutdown, restart functionality).
    #[allow(dead_code)]
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// Check if any subscribers are still listening
    ///
    /// Note: Useful for monitoring/debugging shutdown coordination.
    #[allow(dead_code)]
    pub fn has_subscribers(&self) -> bool {
        self.shutdown_tx.receiver_count() > 0
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ShutdownCoordinator {
    fn clone(&self) -> Self {
        Self {
            shutdown_tx: self.shutdown_tx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_shutdown_signal() {
        let coordinator = ShutdownCoordinator::new();
        let mut rx = coordinator.subscribe();

        // Spawn a task that waits for shutdown
        let task = tokio::spawn(async move {
            rx.recv().await.ok();
            "shutdown received"
        });

        // Trigger shutdown
        coordinator.shutdown();

        // Verify task completed
        let result = timeout(Duration::from_millis(100), task).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().unwrap(), "shutdown received");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let coordinator = ShutdownCoordinator::new();
        let mut rx1 = coordinator.subscribe();
        let mut rx2 = coordinator.subscribe();

        assert!(coordinator.has_subscribers());

        // Trigger shutdown
        coordinator.shutdown();

        // Both should receive signal
        let r1 = timeout(Duration::from_millis(100), rx1.recv()).await;
        let r2 = timeout(Duration::from_millis(100), rx2.recv()).await;

        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[test]
    fn test_clone() {
        let coordinator1 = ShutdownCoordinator::new();
        let coordinator2 = coordinator1.clone();

        let mut rx = coordinator1.subscribe();
        coordinator2.shutdown();

        // Should receive signal from cloned coordinator
        assert!(rx.try_recv().is_ok());
    }
}
