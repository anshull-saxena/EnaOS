use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

use crate::types::events::{EventKind, SystemEvent};

/// The enad event bus.
/// Components publish events; subscribers filter by kind or receive all.
pub struct EventBus {
    /// Per-kind broadcast channels.  Wrapped in Mutex for interior mutability
    /// since EventBus is typically shared behind Arc<EventBus>.
    kind_tx: Mutex<HashMap<EventKind, broadcast::Sender<SystemEvent>>>,
    /// Catch-all channel for subscribers that want everything.
    all_tx: broadcast::Sender<SystemEvent>,
    /// Kept alive so the broadcast channel always has at least one receiver.
    /// Prevents silent message loss before any external subscriber connects.
    _all_rx: broadcast::Receiver<SystemEvent>,
    /// Channel capacity.
    capacity: usize,
}

impl EventBus {
    /// Create a new event bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (all_tx, all_rx) = broadcast::channel(capacity);
        Self {
            kind_tx: Mutex::new(HashMap::new()),
            capacity,
            all_tx,
            _all_rx: all_rx,
        }
    }

    /// Publish an event to both kind-specific and all subscribers.
    pub fn publish(&self, event: SystemEvent) {
        let kind = event.kind.clone();

        // Send to catch-all subscribers.
        let _ = self.all_tx.send(event.clone());

        // Send to kind-specific subscribers.
        if let Some(tx) = self.kind_tx.lock().unwrap().get(&kind) {
            let _ = tx.send(event);
        }
    }

    /// Subscribe to a specific event kind.
    pub fn subscribe(&self, kind: EventKind) -> broadcast::Receiver<SystemEvent> {
        let mut guard = self.kind_tx.lock().unwrap();
        let tx = guard
            .entry(kind)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(self.capacity);
                tx
            });
        tx.subscribe()
    }

    /// Subscribe to all events.
    pub fn subscribe_all(&self) -> broadcast::Receiver<SystemEvent> {
        self.all_tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::events::{EventKind, EventPayload, SystemEvent};
    use tokio::sync::broadcast::error::TryRecvError;

    #[tokio::test]
    async fn test_publish_subscribe_kind() {
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe(EventKind::System);

        let event = SystemEvent::new("test", EventKind::System, EventPayload::SystemActive);
        bus.publish(event.clone());

        let received = rx.try_recv().unwrap();
        assert_eq!(received.kind, EventKind::System);
    }

    #[tokio::test]
    async fn test_catch_all() {
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe_all();

        let event = SystemEvent::new("test", EventKind::Agent, EventPayload::SystemActive);
        bus.publish(event.clone());

        let received = rx.try_recv().unwrap();
        assert_eq!(received.source, "test");
    }

    #[tokio::test]
    async fn test_kind_filtering() {
        let bus = EventBus::new(64);
        let mut rx_window = bus.subscribe(EventKind::Window);

        // Publish a non-Window event — should not appear on window subscriber.
        bus.publish(SystemEvent::new(
            "test",
            EventKind::System,
            EventPayload::SystemActive,
        ));

        let result = rx_window.try_recv();
        assert!(matches!(result, Err(TryRecvError::Empty)));

        // Now publish a Window event.
        bus.publish(SystemEvent::new(
            "test",
            EventKind::Window,
            EventPayload::WindowFocused {
                app: "Alacritty".into(),
                title: "~".into(),
            },
        ));

        let received = rx_window.try_recv().unwrap();
        assert_eq!(received.kind, EventKind::Window);
    }
}
