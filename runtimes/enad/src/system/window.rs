/// Wayland window focus tracking.
///
/// Supports multiple compositor backends:
///   1. GNOME/Mutter — via org.gnome.Shell D-Bus interface
///   2. Sway — via swaymsg JSON IPC
///   3. Hyprland — via Hyprland IPC socket
///   4. Generic wlroots — via wlr-foreign-toplevel (future)
///
/// Detection order:
///   - Check WAYLAND_DISPLAY env for compositor hints
///   - Try GNOME Shell D-Bus first (most common desktop)
///   - Fall back to swaymsg / hyprctl if available
///
/// Publishes WindowFocused, WindowOpened, WindowClosed events.

use std::sync::Arc;

use tracing::info;

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

/// Detect the running compositor type.
fn detect_compositor() -> CompositorType {
    // Check for Sway.
    if std::env::var("SWAYSOCK").is_ok() {
        return CompositorType::Sway;
    }

    // Check for Hyprland.
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return CompositorType::Hyprland;
    }

    // Check for GNOME/Mutter via D-Bus availability.
    // We detect this at runtime by trying the D-Bus connection.
    CompositorType::Unknown
}

#[derive(Debug, Clone, Copy)]
enum CompositorType {
    Sway,
    Hyprland,
    Unknown,
}

/// Run the window focus watcher.
/// Tries the detected compositor backend and falls back to polling.
pub async fn run(bus: Arc<EventBus>) {
    let compositor = detect_compositor();
    info!("Window watcher: detected compositor = {compositor:?}");

    match compositor {
        CompositorType::Sway => run_sway(bus).await,
        CompositorType::Hyprland => run_hyprland(bus).await,
        CompositorType::Unknown => {
            // Try GNOME Shell D-Bus, then fall back to polling.
            if run_gnome_shell(bus.clone()).await {
                return;
            }
            // Fallback: poll xdotool/xdg-based focus.
            run_fallback_poll(bus).await;
        }
    }
}

// ── GNOME Shell (D-Bus) ──────────────────────────────────────────

/// Attempt to connect to GNOME Shell via D-Bus.
/// Returns true if successful (and runs forever), false if unavailable.
async fn run_gnome_shell(bus: Arc<EventBus>) -> bool {
    use zbus::Connection;

    info!("Window watcher: trying GNOME Shell D-Bus...");

    let _conn = match Connection::session().await {
        Ok(c) => c,
        Err(_) => {
            info!("Window watcher: GNOME Shell D-Bus not available");
            return false;
        }
    };

    // Try to get the active window via GNOME Shell introspection.
    // org.gnome.Shell does not expose active window directly via D-Bus.
    // We use a polling approach with xprop-style queries via the shell.
    // For now, use a simpler approach: poll via xprop on X11 or
    // use the GNOME Shell extension IPC.
    //
    // A more robust approach: use the gnome-shell extension that exposes
    // active window via D-Bus. We poll the property.

    info!("Window watcher: GNOME Shell mode active (polling via shell commands)");

    // Poll-based approach for GNOME — check focused window every 2s.
    let mut last_app = String::new();
    let mut last_title = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Try to get the focused window via gdbus or wmctrl.
        if let Ok((app, title)) = get_focused_window_gnome().await {
            if app != last_app || title != last_title {
                info!("Window focused: {app} — {title}");
                bus.publish(SystemEvent::new(
                    "window-watcher",
                    EventKind::Window,
                    EventPayload::WindowFocused {
                        app: app.clone(),
                        title: title.clone(),
                    },
                ));
                last_app = app;
                last_title = title;
            }
        }
    }
}

async fn get_focused_window_gnome() -> Result<(String, String), ()> {
    // Try gdbus call to GNOME Shell for active window info.
    // Fallback: use `xprop -root _NET_ACTIVE_WINDOW` on X11.
    // For Wayland GNOME, we can try `gdbus` to query the shell.

    // Method 1: Try gnome-shell D-Bus via gdbus.
    let output = tokio::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.gnome.Shell",
            "--object-path",
            "/org/gnome/Shell",
            "--method",
            "org.gnome.Shell.Eval",
            "global.display.focus_window.get_wm_class() + '|' + global.display.focus_window.get_title()",
        ])
        .output()
        .await
        .map_err(|_| ())?;

    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout);
        // Output format: (true, 'app|title')
        if let Some(data) = result.strip_prefix("(true, '").and_then(|s| s.strip_suffix("')\n").or(s.strip_suffix("')"))) {
            let parts: Vec<&str> = data.split('|').collect();
            if parts.len() >= 2 {
                let app = parts[0].to_string();
                let title = parts[1].to_string();
                if !app.is_empty() {
                    return Ok((app, title));
                }
            }
        }
    }

    // Method 2: Try `xprop` fallback (works on X11 or XWayland).
    get_focused_window_xprop().await
}

async fn get_focused_window_xprop() -> Result<(String, String), ()> {
    let output = tokio::process::Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .await
        .map_err(|_| ())?;

    if !output.status.success() {
        return Err(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse window ID from: _NET_ACTIVE_WINDOW(WINDOW): window id # 0x4000005
    let window_id = stdout
        .split('#')
        .nth(1)
        .map(|s| s.trim())
        .ok_or(())?;

    // Get WM_CLASS and _NET_WM_NAME for that window.
    let class_output = tokio::process::Command::new("xprop")
        .args(["-id", window_id, "WM_CLASS"])
        .output()
        .await
        .map_err(|_| ())?;

    let name_output = tokio::process::Command::new("xprop")
        .args(["-id", window_id, "_NET_WM_NAME"])
        .output()
        .await
        .map_err(|_| ())?;

    let app = String::from_utf8_lossy(&class_output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .split('"')
        .nth(1)
        .unwrap_or("unknown")
        .to_string();

    let title = String::from_utf8_lossy(&name_output.stdout)
        .splitn(2, '=')
        .nth(1)
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .to_string();

    if app != "unknown" {
        Ok((app, title))
    } else {
        Err(())
    }
}

// ── Sway (JSON IPC) ──────────────────────────────────────────────

async fn run_sway(bus: Arc<EventBus>) {
    info!("Window watcher: using Sway IPC");

    // Subscribe to Sway events via swaymsg.
    // swaymsg -t subscribe [ "window" ]
    // Returns a JSON stream of window events.

    let mut last_app = String::new();
    let mut last_title = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Get focused window via swaymsg.
        if let Ok((app, title)) = get_focused_window_sway().await {
            if app != last_app || title != last_title {
                info!("Window focused: {app} — {title}");
                bus.publish(SystemEvent::new(
                    "window-watcher",
                    EventKind::Window,
                    EventPayload::WindowFocused {
                        app: app.clone(),
                        title: title.clone(),
                    },
                ));
                last_app = app;
                last_title = title;
            }
        }
    }
}

async fn get_focused_window_sway() -> Result<(String, String), ()> {
    let output = tokio::process::Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output()
        .await
        .map_err(|_| ())?;

    if !output.status.success() {
        return Err(());
    }

    let tree = String::from_utf8_lossy(&output.stdout);
    // Parse JSON to find the focused node.
    // Simplified: use jq if available, otherwise parse manually.
    parse_sway_focused(&tree)
}

fn parse_sway_focused(tree: &str) -> Result<(String, String), ()> {
    // Use a simple recursive search for the focused node.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(tree) {
        if let Some(focused) = find_focused_node(&json) {
            let app = focused
                .get("app_id")
                .or_else(|| focused.get("window_properties").and_then(|p| p.get("class")))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let title = focused
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if app != "unknown" {
                return Ok((app, title));
            }
        }
    }
    Err(())
}

fn find_focused_node(node: &serde_json::Value) -> Option<&serde_json::Value> {
    if let Some(focused) = node.get("focused") {
        if focused.as_bool() == Some(true) {
            return Some(node);
        }
    }
    if let Some(nodes) = node.get("nodes").and_then(|n| n.as_array()) {
        for child in nodes {
            if let Some(result) = find_focused_node(child) {
                return Some(result);
            }
        }
    }
    if let Some(nodes) = node.get("floating_nodes").and_then(|n| n.as_array()) {
        for child in nodes {
            if let Some(result) = find_focused_node(child) {
                return Some(result);
            }
        }
    }
    None
}

// ── Hyprland (IPC socket) ────────────────────────────────────────

async fn run_hyprland(bus: Arc<EventBus>) {
    info!("Window watcher: using Hyprland IPC");

    let mut last_app = String::new();
    let mut last_title = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if let Ok((app, title)) = get_focused_window_hyprland().await {
            if app != last_app || title != last_title {
                info!("Window focused: {app} — {title}");
                bus.publish(SystemEvent::new(
                    "window-watcher",
                    EventKind::Window,
                    EventPayload::WindowFocused {
                        app: app.clone(),
                        title: title.clone(),
                    },
                ));
                last_app = app;
                last_title = title;
            }
        }
    }
}

async fn get_focused_window_hyprland() -> Result<(String, String), ()> {
    let instance = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|_| ())?;
    let socket = format!("/tmp/hypr/{instance}/.socket2.sock");

    // Hyprland IPC: send "j/activewindow" to get JSON response.
    use std::os::unix::net::UnixStream;
    use std::io::{Read, Write};

    let mut stream = UnixStream::connect(&socket).map_err(|_| ())?;
    stream.write_all(b"j/activewindow").map_err(|_| ())?;
    stream.flush().map_err(|_| ())?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|_| ())?;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response) {
        let app = json.get("class").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let title = json.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if app != "unknown" {
            return Ok((app, title));
        }
    }

    Err(())
}

// ── Fallback polling ─────────────────────────────────────────────

async fn run_fallback_poll(bus: Arc<EventBus>) {
    info!("Window watcher: using fallback polling (no compositor detected)");

    let mut last_app = String::new();
    let mut last_title = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        if let Ok((app, title)) = get_focused_window_xprop().await {
            if app != last_app || title != last_title {
                info!("Window focused: {app} — {title}");
                bus.publish(SystemEvent::new(
                    "window-watcher",
                    EventKind::Window,
                    EventPayload::WindowFocused {
                        app: app.clone(),
                        title: title.clone(),
                    },
                ));
                last_app = app;
                last_title = title;
            }
        }
    }
}
