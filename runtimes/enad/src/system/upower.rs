/// UPower integration — battery state and power profile monitoring.
///
/// Connects to org.freedesktop.UPower via D-Bus and subscribes to:
///   - Battery percentage changes
///   - Charging/discharging state transitions
///   - Time-to-empty / time-to-full estimates
///   - Power profile changes (via logind)
///
/// D-Bus paths:
///   UPower daemon:     org.freedesktop.UPower
///   Display device:    org.freedesktop.UPower.DisplayDevice
///   Battery devices:   org.freedesktop.UPower.devices.battery_*
///
/// Properties monitored:
///   Percentage (double)
///   State (uint32): 1=charging, 2=discharging, 3=empty, 4=fully-charged, 5=pending-charge, 6=pending-discharge
///   TimeToEmpty (int64, seconds)
///   TimeToFull (int64, seconds)

use std::sync::Arc;

use tracing::{info, warn};
use zbus::{Connection, proxy};

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait DisplayDevice {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;

    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;

    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
}

fn state_label(state: u32) -> &'static str {
    match state {
        1 => "charging",
        2 => "discharging",
        3 => "empty",
        4 => "fully-charged",
        5 => "pending-charge",
        6 => "pending-discharge",
        _ => "unknown",
    }
}

/// Run the UPower battery watcher.
/// Publishes BatteryStatus events when battery state changes.
/// Exits gracefully if UPower is not available.
pub async fn run(bus: Arc<EventBus>) {
    info!("UPower watcher: connecting to system D-Bus...");

    let conn = match Connection::system().await {
        Ok(c) => c,
        Err(e) => {
            warn!("UPower watcher: failed to connect to system D-Bus: {e}");
            return;
        }
    };

    let device = match DisplayDeviceProxy::new(&conn).await {
        Ok(d) => d,
        Err(e) => {
            warn!("UPower watcher: failed to create DisplayDevice proxy: {e}");
            return;
        }
    };

    // Check if a battery is present at all.
    let present = match device.is_present().await {
        Ok(p) => p,
        Err(e) => {
            warn!("UPower watcher: failed to check battery presence: {e}");
            return;
        }
    };

    if !present {
        info!("UPower watcher: no battery detected — exiting");
        return;
    }

    info!("UPower watcher: battery detected, starting monitor");

    // Emit initial state.
    emit_state(&bus, &device).await;

    // Subscribe to property changes.
    let mut receiver = match device.receive_percentage_changed().await {
        Ok(r) => r,
        Err(e) => {
            warn!("UPower watcher: failed to subscribe to percentage changes: {e}");
            return;
        }
    };

    // We also want to watch state changes.
    let mut state_receiver = match device.receive_state_changed().await {
        Ok(r) => r,
        Err(e) => {
            warn!("UPower watcher: failed to subscribe to state changes: {e}");
            return;
        }
    };

    // Listen for either percentage or state changes.
    loop {
        tokio::select! {
            Some(_) = receiver.next() => {
                emit_state(&bus, &device).await;
            }
            Some(_) = state_receiver.next() => {
                emit_state(&bus, &device).await;
            }
        }
    }
}

async fn emit_state(bus: &EventBus, device: &DisplayDeviceProxy<'_>) {
    let percentage = device.percentage().await.unwrap_or(0.0);
    let state = device.state().await.unwrap_or(0);
    let time_to_empty = device.time_to_empty().await.ok();
    let time_to_full = device.time_to_full().await.ok();

    // Only emit if time values are meaningful (> 0).
    let tte = time_to_empty.filter(|&v| v > 0);
    let ttf = time_to_full.filter(|&v| v > 0);

    info!(
        "UPower: battery {:.1}% — {}",
        percentage,
        state_label(state)
    );

    bus.publish(SystemEvent::new(
        "upower",
        EventKind::System,
        EventPayload::BatteryStatus {
            percentage,
            state: state_label(state).to_string(),
            time_to_empty: tte,
            time_to_full: ttf,
        },
    ));
}
