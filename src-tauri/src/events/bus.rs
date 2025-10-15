use super::types::{EventSequence, SessionEvent, SessionEventPayload};
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

pub type EventReceiver = broadcast::Receiver<SessionEvent>;
pub type EventSender = broadcast::Sender<SessionEvent>;

/// Event bus for distributing session events
#[derive(Clone, Debug)]
pub struct EventBus {
    sender: EventSender,
    sequence: Arc<AtomicU64>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            sequence: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Publish an event (returns sequence number)
    pub fn publish(
        &self,
        provider: &str,
        payload: SessionEventPayload,
    ) -> Result<EventSequence, String> {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);

        let event = SessionEvent {
            sequence,
            timestamp: Utc::now(),
            provider: provider.to_string(),
            payload,
        };

        self.sender
            .send(event)
            .map(|_| sequence)
            .map_err(|e| format!("Failed to publish event: {}", e))
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> EventReceiver {
        self.sender.subscribe()
    }

    /// Get current sequence number
    pub fn current_sequence(&self) -> EventSequence {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Get number of active receivers
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(100);
        let mut rx = bus.subscribe();

        let payload = SessionEventPayload::SessionChanged {
            session_id: "test-session".to_string(),
            project_name: "test-project".to_string(),
            file_path: PathBuf::from("/tmp/test.json"),
            file_size: 1024,
        };

        let seq = bus.publish("claude", payload.clone()).unwrap();
        assert_eq!(seq, 1);

        let event = rx.recv().await.unwrap();
        assert_eq!(event.sequence, 1);
        assert_eq!(event.provider, "claude");
        assert_eq!(event.session_id(), "test-session");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(100);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let payload = SessionEventPayload::SessionChanged {
            session_id: "test-session".to_string(),
            project_name: "test-project".to_string(),
            file_path: PathBuf::from("/tmp/test.json"),
            file_size: 2048,
        };

        bus.publish("gemini", payload).unwrap();

        let event1 = rx1.recv().await.unwrap();
        let event2 = rx2.recv().await.unwrap();

        assert_eq!(event1.sequence, event2.sequence);
        assert_eq!(event1.provider, "gemini");
        assert_eq!(event2.provider, "gemini");
    }

    #[test]
    fn test_sequence_ordering() {
        let bus = EventBus::new(100);
        let _rx = bus.subscribe(); // Keep receiver alive to prevent channel from closing

        let seq1 = bus
            .publish(
                "claude",
                SessionEventPayload::SessionChanged {
                    session_id: "s1".to_string(),
                    project_name: "p1".to_string(),
                    file_path: PathBuf::from("/tmp/1.json"),
                    file_size: 100,
                },
            )
            .unwrap();

        let seq2 = bus
            .publish(
                "gemini",
                SessionEventPayload::SessionChanged {
                    session_id: "s2".to_string(),
                    project_name: "p2".to_string(),
                    file_path: PathBuf::from("/tmp/2.json"),
                    file_size: 200,
                },
            )
            .unwrap();

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
    }
}
