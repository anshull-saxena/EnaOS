use std::sync::Arc;
use tokio::signal;
use tracing::info;

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

/// System hooks controller.
/// Manages OS-level hooks: signal handling.
///
/// Window focus, clipboard, audio, and other desktop integrations
/// are now handled by the `system` module subsystems.
pub struct SystemHooks {
    bus: Arc<EventBus>,
}

impl SystemHooks {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self { bus }
    }

    /// Listen for SIGINT/SIGTERM and publish a shutdown event.
    /// Returns when a signal is received.
    pub async fn wait_for_shutdown(&self) {
        info!("System hooks: waiting for shutdown signal...");

        // Handle SIGINT (Ctrl+C).
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received SIGINT");

                self.bus.publish(SystemEvent::new(
                    "hooks",
                    EventKind::System,
                    EventPayload::SystemSleep,
                ));
            }
            Err(e) => {
                tracing::error!("Failed to listen for SIGINT: {e}");
            }
        }
    }
}
