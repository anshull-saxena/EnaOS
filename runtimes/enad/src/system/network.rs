/// NetworkManager integration — connectivity and SSID monitoring.
///
/// Connects to org.freedesktop.NetworkManager via D-Bus and subscribes to:
///   - Global connectivity state changes
///   - Active connection changes (WiFi SSID, Ethernet)
///   - Device state changes (up/down)
///
/// D-Bus paths:
///   NM daemon:       org.freedesktop.NetworkManager
///   NM state:        org.freedesktop.NetworkManager.State
///   Active conn:     org.freedesktop.NetworkManager.Connection.Active
///
/// NM State values:
///   10 = asleep, 20 = disconnected, 30 = disconnecting,
///   40 = connecting, 50 = connected-local, 60 = connected-site,
///   70 = connected-global
use std::sync::Arc;

use tracing::{info, warn};
use zbus::{Connection, proxy};

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManagerInterface {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
trait WirelessDevice {
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager"
)]
trait AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;

    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;
}

fn state_label(state: u32) -> (&'static str, bool) {
    match state {
        10 => ("asleep", false),
        20 => ("disconnected", false),
        30 => ("disconnecting", false),
        40 => ("connecting", false),
        50 => ("connected-local", false),
        60 => ("connected-site", true),
        70 => ("connected-global", true),
        _ => ("unknown", false),
    }
}

/// Decode NM SSID (Vec<u8>) to String.
fn decode_ssid(raw: Vec<u8>) -> String {
    String::from_utf8_lossy(&raw).to_string()
}

/// Run the NetworkManager connectivity watcher.
/// Publishes NetworkStatus events on state changes.
pub async fn run(bus: Arc<EventBus>) {
    info!("NetworkManager watcher: connecting to system D-Bus...");

    let conn = match Connection::system().await {
        Ok(c) => c,
        Err(e) => {
            warn!("NetworkManager watcher: failed to connect to system D-Bus: {e}");
            return;
        }
    };

    let nm = match NetworkManagerInterfaceProxy::new(&conn).await {
        Ok(n) => n,
        Err(e) => {
            warn!("NetworkManager watcher: failed to create NM proxy: {e}");
            return;
        }
    };

    info!("NetworkManager watcher: monitoring connectivity");

    // Emit initial state.
    emit_state(&bus, &conn, &nm).await;

    // Subscribe to state changes.
    let mut state_rx = match nm.receive_state_changed().await {
        Ok(r) => r,
        Err(e) => {
            warn!("NetworkManager watcher: failed to subscribe to state changes: {e}");
            return;
        }
    };

    let mut active_rx = match nm.receive_active_connections_changed().await {
        Ok(r) => r,
        Err(e) => {
            warn!("NetworkManager watcher: failed to subscribe to active connections: {e}");
            return;
        }
    };

    loop {
        tokio::select! {
            Some(_) = state_rx.next() => {
                emit_state(&bus, &conn, &nm).await;
            }
            Some(_) = active_rx.next() => {
                emit_state(&bus, &conn, &nm).await;
            }
        }
    }
}

async fn emit_state(bus: &EventBus, conn: &Connection, nm: &NetworkManagerInterfaceProxy<'_>) {
    let state = nm.state().await.unwrap_or(20);
    let (label, connected) = state_label(state);

    // Try to get SSID from active WiFi connection.
    let mut ssid: Option<String> = None;
    let mut strength: Option<u8> = None;

    if connected {
        if let Ok(paths) = nm.active_connections().await {
            for path in paths {
                // Build ActiveConnection proxy from path.
                if let Ok(builder) = ActiveConnectionProxy::builder(conn) {
                    if let Ok(builder) = builder.path(path.clone()) {
                        if let Ok(ac) = builder.build().await {
                            if let Ok(devices) = ac.devices().await {
                                for dev_path in devices {
                                    // Build WirelessDevice proxy from device path.
                                    if let Ok(builder) = WirelessDeviceProxy::builder(conn) {
                                        if let Ok(builder) = builder.path(dev_path.clone()) {
                                            if let Ok(wdev) = builder.build().await {
                                                if let Ok(ap_path) =
                                                    wdev.active_access_point().await
                                                {
                                                    if ap_path.as_str() != "/" {
                                                        // Build AccessPoint proxy from AP path.
                                                        if let Ok(builder) =
                                                            AccessPointProxy::builder(conn)
                                                        {
                                                            if let Ok(builder) =
                                                                builder.path(ap_path)
                                                            {
                                                                if let Ok(ap) =
                                                                    builder.build().await
                                                                {
                                                                    if let Ok(raw_ssid) =
                                                                        ap.ssid().await
                                                                    {
                                                                        ssid = Some(decode_ssid(
                                                                            raw_ssid,
                                                                        ));
                                                                    }
                                                                    if let Ok(s) =
                                                                        ap.strength().await
                                                                    {
                                                                        strength = Some(s);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    info!(
        "NetworkManager: {label}{}",
        ssid.as_ref().map(|s| format!(" — {s}")).unwrap_or_default()
    );

    bus.publish(SystemEvent::new(
        "networkmanager",
        EventKind::System,
        EventPayload::NetworkStatus {
            connected,
            ssid,
            strength,
        },
    ));
}
