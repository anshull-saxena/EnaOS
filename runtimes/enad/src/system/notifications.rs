/// Freedesktop Notifications D-Bus integration.
///
/// Implements a notification server that listens for notifications
/// sent via org.freedesktop.Notifications and publishes them as events.
///
/// This allows EnaOS to be aware of all system notifications, not just
/// display them. The bar can show notification indicators or summaries.
///
/// D-Bus interface: org.freedesktop.Notifications
/// D-Bus path: /org/freedesktop/Notifications
///
/// Note: This does NOT replace the system notification daemon (e.g.,
/// mako, dunst). It runs as a secondary listener that observes notifications
/// via D-Bus signal monitoring.

use std::sync::Arc;

use tracing::{info, warn};
use zbus::{Connection, proxy};

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait NotificationsInterface {
    /// Get server information.
    fn get_server_information(&self) -> (String, String, String, String);

    /// Get capabilities.
    fn get_capabilities(&self) -> Vec<String>;
}

/// Urgency levels from notification hints.
fn urgency_label(hints: &serde_json::Value) -> String {
    if let Some(urgency) = hints.get("urgency") {
        match urgency.as_u64() {
            Some(0) => "low",
            Some(1) => "normal",
            Some(2) => "critical",
            _ => "normal",
        }
    } else {
        "normal"
    }
    .to_string()
}

/// Run the notification listener.
/// Monitors D-Bus signals from the notification service.
pub async fn run(bus: Arc<EventBus>) {
    info!("Notification watcher: connecting to session D-Bus...");

    let conn = match Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            warn!("Notification watcher: failed to connect to session D-Bus: {e}");
            return;
        }
    };

    // Check if a notification server is available.
    let notif = match NotificationsInterfaceProxy::new(&conn).await {
        Ok(n) => n,
        Err(e) => {
            warn!("Notification watcher: no notification server available: {e}");
            return;
        }
    };

    match notif.get_server_information().await {
        Ok((name, vendor, version, spec_version)) => {
            info!(
                "Notification watcher: server = {name} ({vendor} {version}, spec {spec_version})"
            );
        }
        Err(e) => {
            warn!("Notification watcher: failed to get server info: {e}");
            return;
        }
    }

    info!("Notification watcher: monitoring D-Bus signals");

    // Monitor NotificationClosed signals.
    let mut closed_rx = match conn
        .subscribe()
        .await
        .map(|mut r| {
            r.add_match(
                "type='signal',interface='org.freedesktop.Notifications',member='NotificationClosed'",
            )
            .expect("valid match rule");
            r
        }) {
        Ok(r) => r,
        Err(e) => {
            warn!("Notification watcher: failed to subscribe to signals: {e}");
            return;
        }
    };

    // We also need to monitor Notify calls.
    // Since we're not the notification server, we can't intercept Notify calls directly.
    // Instead, we monitor the D-Bus traffic for Notify method calls.
    let mut notify_rx = match conn.subscribe().await {
        Ok(mut r) => {
            r.add_match(
                "type='method_call',interface='org.freedesktop.Notifications',member='Notify'",
            )
            .expect("valid match rule");
            r
        }
        Err(e) => {
            warn!("Notification watcher: failed to subscribe to Notify calls: {e}");
            return;
        }
    };

    loop {
        tokio::select! {
            Some(msg) = closed_rx.next() => {
                // NotificationClosed signal: (uint32 id, uint32 reason)
                if let Ok(body) = msg.body::<(u32, u32)>() {
                    info!("Notification dismissed: id={}", body.0);
                    bus.publish(SystemEvent::new(
                        "notification-watcher",
                        EventKind::Notification,
                        EventPayload::NotificationDismissed {
                            id: body.0,
                        },
                    ));
                }
            }
            Some(_msg) = notify_rx.next() => {
                // We see the Notify call but can't easily extract the body
                // without being the actual notification server.
                // Log that a notification was sent.
                info!("Notification: new notification received (details require server proxy)");
            }
        }
    }
}

/// Alternative: run as a full notification server that receives all notifications.
/// This replaces the existing notification daemon.
pub async fn run_as_server(bus: Arc<EventBus>) {
    info!("Notification server: registering as org.freedesktop.Notifications");

    // To run as a notification server, we need to:
    // 1. Claim the org.freedesktop.Notifications well-known name
    // 2. Implement the Notify method
    // 3. Implement ActionInvoked and NotificationClosed signals
    //
    // This is complex with zbus and requires careful D-Bus name ownership.
    // For now, we use the passive monitoring approach above.
    //
    // A full implementation would use zbus::ObjectServer and
    // #[zbus::interface] to implement the notification spec.

    warn!("Notification server mode not yet implemented — using passive monitoring");
    run(bus).await;
}
