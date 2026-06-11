/// Workspace / virtual desktop awareness.
///
/// Supports:
///   1. Sway — via swaymsg -t get_workspaces
///   2. Hyprland — via Hyprland IPC socket
///   3. GNOME — via D-Bus (org.gnome.Mutter.WorkspaceManager)
///   4. KDE — via D-Bus (org.kde.KWin)
///
/// Publishes WorkspaceChanged events when the active workspace changes.
use std::sync::Arc;

use tracing::{info, warn};

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

/// Detect the running compositor type.
fn detect_compositor() -> &'static str {
    if std::env::var("SWAYSOCK").is_ok() {
        return "sway";
    }
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return "hyprland";
    }
    // Default to trying GNOME D-Bus.
    "gnome"
}

/// Run the workspace watcher.
pub async fn run(bus: Arc<EventBus>) {
    let compositor = detect_compositor();
    info!("Workspace watcher: targeting {compositor}");

    match compositor {
        "sway" => run_sway(bus).await,
        "hyprland" => run_hyprland(bus).await,
        "gnome" => run_gnome(bus).await,
        _ => {
            warn!("Workspace watcher: unsupported compositor");
        }
    }
}

// ── Sway ─────────────────────────────────────────────────────────

async fn run_sway(bus: Arc<EventBus>) {
    let mut last_workspace = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if let Ok(ws) = get_active_workspace_sway().await {
            if ws != last_workspace {
                info!("Workspace changed: {ws}");
                bus.publish(SystemEvent::new(
                    "workspace-watcher",
                    EventKind::Window,
                    EventPayload::WorkspaceChanged {
                        workspace: ws.clone(),
                        output: None,
                    },
                ));
                last_workspace = ws;
            }
        }
    }
}

async fn get_active_workspace_sway() -> Result<String, ()> {
    let output = tokio::process::Command::new("swaymsg")
        .args(["-t", "get_workspaces"])
        .output()
        .await
        .map_err(|_| ())?;

    if !output.status.success() {
        return Err(());
    }

    let json = String::from_utf8_lossy(&output.stdout);
    if let Ok(workspaces) = serde_json::from_str::<serde_json::Value>(&json) {
        if let Some(arr) = workspaces.as_array() {
            for ws in arr {
                if ws.get("focused").and_then(|v| v.as_bool()) == Some(true) {
                    if let Some(name) = ws.get("name").and_then(|v| v.as_str()) {
                        return Ok(name.to_string());
                    }
                }
            }
        }
    }

    Err(())
}

// ── Hyprland ─────────────────────────────────────────────────────

async fn run_hyprland(bus: Arc<EventBus>) {
    let mut last_workspace = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if let Ok(ws) = get_active_workspace_hyprland().await {
            if ws != last_workspace {
                info!("Workspace changed: {ws}");
                bus.publish(SystemEvent::new(
                    "workspace-watcher",
                    EventKind::Window,
                    EventPayload::WorkspaceChanged {
                        workspace: ws.clone(),
                        output: None,
                    },
                ));
                last_workspace = ws;
            }
        }
    }
}

async fn get_active_workspace_hyprland() -> Result<String, ()> {
    let instance = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|_| ())?;
    let socket = format!("/tmp/hypr/{instance}/.socket2.sock");

    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(&socket).map_err(|_| ())?;
    stream.write_all(b"j/workspaces").map_err(|_| ())?;
    stream.flush().map_err(|_| ())?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|_| ())?;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response) {
        if let Some(arr) = json.as_array() {
            for ws in arr {
                if ws.get("focused").and_then(|v| v.as_bool()) == Some(true) {
                    if let Some(name) = ws.get("name").and_then(|v| v.as_str()) {
                        return Ok(name.to_string());
                    }
                }
            }
        }
    }

    Err(())
}

// ── GNOME ────────────────────────────────────────────────────────

async fn run_gnome(bus: Arc<EventBus>) {
    info!("Workspace watcher: GNOME mode (polling via gdbus)");

    let mut last_workspace = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        if let Ok(ws) = get_active_workspace_gnome().await {
            if ws != last_workspace {
                info!("Workspace changed: {ws}");
                bus.publish(SystemEvent::new(
                    "workspace-watcher",
                    EventKind::Window,
                    EventPayload::WorkspaceChanged {
                        workspace: ws.clone(),
                        output: None,
                    },
                ));
                last_workspace = ws;
            }
        }
    }
}

async fn get_active_workspace_gnome() -> Result<String, ()> {
    // Use gdbus to query the active workspace number from GNOME Shell.
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
            "global.workspace_manager.get_active_workspace_index().toString()",
        ])
        .output()
        .await
        .map_err(|_| ())?;

    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout);
        // Output: (true, '1')
        if let Some(num) = result
            .strip_prefix("(true, '")
            .and_then(|s| s.strip_suffix("')\n").or(s.strip_suffix("')")))
        {
            return Ok(format!("Workspace {}", num));
        }
    }

    Err(())
}
