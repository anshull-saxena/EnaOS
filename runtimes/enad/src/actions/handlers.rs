/// Action handlers — Linux desktop control implementations.
///
/// Each handler executes a specific action type using native Linux tools.
/// All handlers are async and return Result<String, String> for status reporting.

use tracing::info;

use crate::actions::types::ActionType;

/// Execute an action and return a status message.
pub async fn execute(action: &ActionType) -> Result<String, String> {
    match action {
        ActionType::OpenApp { app } => open_app(app).await,
        ActionType::OpenUrl { url } => open_url(url).await,
        ActionType::FocusWindow { app, title } => focus_window(app.as_deref(), title.as_deref()).await,
        ActionType::LaunchCommand { command, args } => launch_command(command, args).await,
        ActionType::SwitchWorkspace { workspace } => switch_workspace(workspace).await,
        ActionType::SearchFiles { query, path } => search_files(query, path.as_deref()).await,
        ActionType::MediaControl { action } => media_control(action).await,
        ActionType::ClipboardSet { text } => clipboard_set(text).await,
        ActionType::ReadWindowTitle => read_window_title().await,
        ActionType::Notify { title, body } => notify(title, body).await,
    }
}

// ── Open Application ─────────────────────────────────────────────

async fn open_app(app: &str) -> Result<String, String> {
    info!("Action: opening app '{app}'");

    // Try gio launch first (GNOME/modern).
    let result = tokio::process::Command::new("gio")
        .args(["launch", app])
        .output()
        .await;

    if let Ok(output) = result {
        if output.status.success() {
            return Ok(format!("Launched {app}"));
        }
    }

    // Fallback: try xdg-open.
    let result = tokio::process::Command::new("xdg-open")
        .arg(app)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => Ok(format!("Opened {app}")),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to open {app}: {stderr}"))
        }
        Err(e) => Err(format!("Failed to open {app}: {e}")),
    }
}

// ── Open URL ─────────────────────────────────────────────────────

async fn open_url(url: &str) -> Result<String, String> {
    info!("Action: opening URL '{url}'");

    let result = tokio::process::Command::new("xdg-open")
        .arg(url)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => Ok(format!("Opened {url}")),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to open URL: {stderr}"))
        }
        Err(e) => Err(format!("Failed to open URL: {e}")),
    }
}

// ── Focus Window ─────────────────────────────────────────────────

async fn focus_window(app: Option<&str>, title: Option<&str>) -> Result<String, String> {
    info!("Action: focusing window app={app:?} title={title:?}");

    // Try swaymsg first (Sway compositor).
    if std::env::var("SWAYSOCK").is_ok() {
        return focus_window_sway(app, title).await;
    }

    // Try Hyprland.
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return focus_window_hyprland(app, title).await;
    }

    // Fallback: wmctrl.
    focus_window_wmctrl(app, title).await
}

async fn focus_window_sway(app: Option<&str>, title: Option<&str>) -> Result<String, String> {
    let output = tokio::process::Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output()
        .await
        .map_err(|e| format!("swaymsg failed: {e}"))?;

    let tree = String::from_utf8_lossy(&output.stdout);
    if let Some(window_id) = find_window_in_sway_tree(&tree, app, title) {
        let result = tokio::process::Command::new("swaymsg")
            .args(["[con_id=", &window_id, "] focus"])
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                let label = app.or(title).unwrap_or("window");
                Ok(format!("Focused {label}"))
            }
            Ok(output) => Err(format!("Failed to focus: {}", String::from_utf8_lossy(&output.stderr))),
            Err(e) => Err(format!("Focus command failed: {e}")),
        }
    } else {
        Err("Window not found".to_string())
    }
}

fn find_window_in_sway_tree(tree: &str, app: Option<&str>, title: Option<&str>) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(tree) {
        return search_sway_node(&json, app, title);
    }
    None
}

fn search_sway_node(node: &serde_json::Value, app: Option<&str>, title: Option<&str>) -> Option<String> {
    let node_app = node.get("app_id").and_then(|v| v.as_str()).unwrap_or("");
    let node_title = node.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let node_id = node.get("id").and_then(|v| v.as_u64()).unwrap_or(0);

    let match_app = app.map(|a| node_app.contains(a)).unwrap_or(true);
    let match_title = title.map(|t| node_title.contains(t)).unwrap_or(true);

    if match_app && match_title && node_id > 0 {
        return Some(node_id.to_string());
    }

    if let Some(nodes) = node.get("nodes").and_then(|n| n.as_array()) {
        for child in nodes {
            if let Some(result) = search_sway_node(child, app, title) {
                return Some(result);
            }
        }
    }

    None
}

async fn focus_window_hyprland(app: Option<&str>, title: Option<&str>) -> Result<String, String> {
    let instance = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| "HYPRLAND_INSTANCE_SIGNATURE not set".to_string())?;
    let socket = format!("/tmp/hypr/{instance}/.socket2.sock");

    use std::os::unix::net::UnixStream;
    use std::io::{Read, Write};

    let mut stream = UnixStream::connect(&socket)
        .map_err(|e| format!("Failed to connect to Hyprland socket: {e}"))?;

    stream.write_all(b"j/clients")
        .map_err(|e| format!("Hyprland IPC write failed: {e}"))?;
    stream.flush()
        .map_err(|e| format!("Hyprland IPC flush failed: {e}"))?;

    let mut response = String::new();
    stream.read_to_string(&mut response)
        .map_err(|e| format!("Hyprland IPC read failed: {e}"))?;

    if let Ok(clients) = serde_json::from_str::<serde_json::Value>(&response) {
        if let Some(arr) = clients.as_array() {
            for client in arr {
                let client_app = client.get("class").and_then(|v| v.as_str()).unwrap_or("");
                let client_title = client.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let client_addr = client.get("address").and_then(|v| v.as_str()).unwrap_or("");

                let match_app = app.map(|a| client_app.contains(a)).unwrap_or(true);
                let match_title = title.map(|t| client_title.contains(t)).unwrap_or(true);

                if match_app && match_title && !client_addr.is_empty() {
                    let result = tokio::process::Command::new("hyprctl")
                        .args(["dispatch", "focuswindow", "address", client_addr])
                        .output()
                        .await;

                    match result {
                        Ok(output) if output.status.success() => {
                            let label = app.or(title).unwrap_or("window");
                            return Ok(format!("Focused {label}"));
                        }
                        Ok(output) => return Err(format!("Focus failed: {}", String::from_utf8_lossy(&output.stderr))),
                        Err(e) => return Err(format!("Focus command failed: {e}")),
                    }
                }
            }
        }
    }

    Err("Window not found".to_string())
}

async fn focus_window_wmctrl(app: Option<&str>, title: Option<&str>) -> Result<String, String> {
    let search = title.or(app).ok_or("Must specify app or title".to_string())?;

    let output = tokio::process::Command::new("wmctrl")
        .args(["-l"])
        .output()
        .await
        .map_err(|e| format!("wmctrl failed: {e}"))?;

    let lines = String::from_utf8_lossy(&output.stdout);
    for line in lines.lines() {
        if line.to_lowercase().contains(&search.to_lowercase()) {
            let window_id = line.split_whitespace().next().unwrap_or("");
            let result = tokio::process::Command::new("wmctrl")
                .args(["-i", "-a", window_id])
                .output()
                .await;

            match result {
                Ok(output) if output.status.success() => return Ok(format!("Focused {search}")),
                _ => return Err(format!("Failed to focus {search}")),
            }
        }
    }

    Err(format!("No window matching '{search}' found"))
}

// ── Launch Command ───────────────────────────────────────────────

async fn launch_command(command: &str, args: &[String]) -> Result<String, String> {
    info!("Action: launching command '{command}' with args {args:?}");

    let result = tokio::process::Command::new(command)
        .args(args)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.trim().to_string().is_empty()
                .then(|| format!("Command executed: {command}"))
                .unwrap_or_else(|| stdout.trim().to_string()))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Command failed: {stderr}"))
        }
        Err(e) => Err(format!("Failed to execute {command}: {e}")),
    }
}

// ── Switch Workspace ─────────────────────────────────────────────

async fn switch_workspace(workspace: &str) -> Result<String, String> {
    info!("Action: switching to workspace '{workspace}'");

    // Try swaymsg first.
    if std::env::var("SWAYSOCK").is_ok() {
        let result = tokio::process::Command::new("swaymsg")
            .args(["workspace", workspace])
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => return Ok(format!("Switched to {workspace}")),
            Ok(output) => return Err(format!("Failed: {}", String::from_utf8_lossy(&output.stderr))),
            Err(e) => return Err(format!("swaymsg failed: {e}")),
        }
    }

    // Try Hyprland.
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        let result = tokio::process::Command::new("hyprctl")
            .args(["dispatch", "workspace", workspace])
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => return Ok(format!("Switched to {workspace}")),
            Ok(output) => return Err(format!("Failed: {}", String::from_utf8_lossy(&output.stderr))),
            Err(e) => return Err(format!("hyprctl failed: {e}")),
        }
    }

    // Try GNOME via gdbus.
    let result = tokio::process::Command::new("gdbus")
        .args([
            "call", "--session",
            "--dest", "org.gnome.Shell",
            "--object-path", "/org/gnome/Shell",
            "--method", "org.gnome.Shell.Eval",
            &format!("global.workspace_manager.get_workspace_by_name('{workspace}', null)?.activate(global.get_current_time())"),
        ])
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => Ok(format!("Switched to {workspace}")),
        _ => Err("Workspace switching not supported on this compositor".to_string()),
    }
}

// ── Search Files ─────────────────────────────────────────────────

async fn search_files(query: &str, path: Option<&str>) -> Result<String, String> {
    info!("Action: searching files for '{query}'");

    let search_path = path.unwrap_or("/home");

    // Try fd first (faster).
    let result = tokio::process::Command::new("fd")
        .args(["-i", "--max-depth", "3", query, search_path])
        .output()
        .await;

    if let Ok(output) = result {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().take(20).collect();
            if lines.is_empty() {
                return Ok(format!("No files matching '{query}'"));
            }
            return Ok(format!("Found {} files:\n{}", lines.len(), lines.join("\n")));
        }
    }

    // Fallback: find.
    let result = tokio::process::Command::new("find")
        .args([search_path, "-maxdepth", "3", "-iname", &format!("*{query}*")])
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().take(20).collect();
            if lines.is_empty() {
                Ok(format!("No files matching '{query}'"))
            } else {
                Ok(format!("Found {} files:\n{}", lines.len(), lines.join("\n")))
            }
        }
        Ok(output) => Err(format!("Search failed: {}", String::from_utf8_lossy(&output.stderr))),
        Err(e) => Err(format!("Search command failed: {e}")),
    }
}

// ── Media Control ────────────────────────────────────────────────

async fn media_control(action: &str) -> Result<String, String> {
    info!("Action: media control '{action}'");

    let result = tokio::process::Command::new("playerctl")
        .arg(action)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => Ok(format!("Media: {action}")),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No players") {
                Err("No media players found".to_string())
            } else {
                Err(format!("Media control failed: {stderr}"))
            }
        }
        Err(e) => Err(format!("playerctl not found: {e}")),
    }
}

// ── Clipboard Set ────────────────────────────────────────────────

async fn clipboard_set(text: &str) -> Result<String, String> {
    info!("Action: setting clipboard");

    // Try wl-copy first (Wayland).
    let result = tokio::process::Command::new("wl-copy")
        .arg(text)
        .output()
        .await;

    if let Ok(output) = result {
        if output.status.success() {
            return Ok("Clipboard updated".to_string());
        }
    }

    // Fallback: xclip.
    use std::process::Stdio;
    let mut child = tokio::process::Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("xclip failed: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(text.as_bytes()).await
            .map_err(|e| format!("Failed to write to xclip: {e}"))?;
    }

    let output = child.wait_with_output().await
        .map_err(|e| format!("xclip wait failed: {e}"))?;

    if output.status.success() {
        Ok("Clipboard updated".to_string())
    } else {
        Err(format!("Failed to set clipboard: {}", String::from_utf8_lossy(&output.stderr)))
    }
}

// ── Read Window Title ────────────────────────────────────────────

async fn read_window_title() -> Result<String, String> {
    info!("Action: reading active window title");

    // Try swaymsg first.
    if std::env::var("SWAYSOCK").is_ok() {
        let output = tokio::process::Command::new("swaymsg")
            .args(["-t", "get_tree"])
            .output()
            .await
            .map_err(|e| format!("swaymsg failed: {e}"))?;

        let tree = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&tree) {
            if let Some(focused) = find_focused_sway_node(&json) {
                let app = focused.get("app_id").and_then(|v| v.as_str()).unwrap_or("");
                let title = focused.get("name").and_then(|v| v.as_str()).unwrap_or("");
                return Ok(format!("{app} — {title}"));
            }
        }
    }

    // Try Hyprland.
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        let instance = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap();
        let socket = format!("/tmp/hypr/{instance}/.socket2.sock");

        use std::os::unix::net::UnixStream;
        use std::io::{Read, Write};

        let mut stream = UnixStream::connect(&socket)
            .map_err(|e| format!("Hyprland socket: {e}"))?;
        stream.write_all(b"j/activewindow")
            .map_err(|e| format!("Hyprland write: {e}"))?;
        stream.flush()
            .map_err(|e| format!("Hyprland flush: {e}"))?;

        let mut response = String::new();
        stream.read_to_string(&mut response)
            .map_err(|e| format!("Hyprland read: {e}"))?;

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response) {
            let class = json.get("class").and_then(|v| v.as_str()).unwrap_or("");
            let title = json.get("title").and_then(|v| v.as_str()).unwrap_or("");
            return Ok(format!("{class} — {title}"));
        }
    }

    // Fallback: xprop.
    let output = tokio::process::Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .await
        .map_err(|e| format!("xprop failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(window_id) = stdout.split('#').nth(1).map(|s| s.trim()) {
        let title_output = tokio::process::Command::new("xprop")
            .args(["-id", window_id, "_NET_WM_NAME"])
            .output()
            .await;

        if let Ok(out) = title_output {
            let title = String::from_utf8_lossy(&out.stdout)
                .splitn(2, '=')
                .nth(1)
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string();

            if !title.is_empty() {
                return Ok(title);
            }
        }
    }

    Err("Could not determine active window".to_string())
}

fn find_focused_sway_node(node: &serde_json::Value) -> Option<&serde_json::Value> {
    if node.get("focused").and_then(|v| v.as_bool()) == Some(true) {
        return Some(node);
    }
    if let Some(nodes) = node.get("nodes").and_then(|n| n.as_array()) {
        for child in nodes {
            if let Some(result) = find_focused_sway_node(child) {
                return Some(result);
            }
        }
    }
    None
}

// ── Notify ───────────────────────────────────────────────────────

async fn notify(title: &str, body: &str) -> Result<String, String> {
    info!("Action: sending notification '{title}'");

    // Try notify-send first.
    let result = tokio::process::Command::new("notify-send")
        .args([title, body])
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => Ok(format!("Notification: {title}")),
        Ok(output) => Err(format!("Notification failed: {}", String::from_utf8_lossy(&output.stderr))),
        Err(_) => {
            // Fallback: gdbus to fdo.Notifications.
            let escaped_body = body.replace('"', "\\\"");
            let result = tokio::process::Command::new("gdbus")
                .args([
                    "call", "--session",
                    "--dest", "org.freedesktop.Notifications",
                    "--object-path", "/org/freedesktop/Notifications",
                    "--method", "org.freedesktop.Notifications.Notify",
                    "enaos", "0", "dialog-information",
                    title, &escaped_body,
                    "[]", "{}", "0",
                ])
                .output()
                .await;

            match result {
                Ok(output) if output.status.success() => Ok(format!("Notification: {title}")),
                _ => Err("Notification system not available".to_string()),
            }
        }
    }
}
