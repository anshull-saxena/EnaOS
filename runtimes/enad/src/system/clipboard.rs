/// Clipboard monitoring via wl-clipboard (Wayland) or xclip (X11).
///
/// Watches the clipboard for changes and publishes ClipboardUpdated events.
/// Only stores a short preview — never the full clipboard content.
///
/// Strategy:
///   - Poll `wl-paste --watch` on Wayland (event-driven)
///   - Fall back to polling `xclip -selection clipboard -o` on X11
///
/// Privacy: only the first 80 chars of text content are included in the event.

use std::sync::Arc;

use tracing::{info, warn};

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

const PREVIEW_MAX: usize = 80;

/// Detect clipboard tool availability.
fn detect_clipboard_tool() -> ClipboardTool {
    if command_exists("wl-paste") {
        return ClipboardTool::WlClipboard;
    }
    if command_exists("xclip") {
        return ClipboardTool::Xclip;
    }
    ClipboardTool::None
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Debug)]
enum ClipboardTool {
    WlClipboard,
    Xclip,
    None,
}

/// Run the clipboard watcher.
pub async fn run(bus: Arc<EventBus>) {
    let tool = detect_clipboard_tool();
    info!("Clipboard watcher: tool = {tool:?}");

    match tool {
        ClipboardTool::WlClipboard => run_wl_paste_watch(bus).await,
        ClipboardTool::Xclip => run_xclip_poll(bus).await,
        ClipboardTool::None => {
            warn!("Clipboard watcher: no clipboard tool available (install wl-clipboard or xclip)");
        }
    }
}

// ── wl-clipboard (event-driven via --watch) ──────────────────────

async fn run_wl_paste_watch(bus: Arc<EventBus>) {
    info!("Clipboard watcher: using wl-paste --watch");

    // wl-paste --watch runs a command each time clipboard changes.
    // We use it to trigger a callback that reads the clipboard.
    // Since --watch forks, we instead poll with wl-paste directly.

    let mut last_hash = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        if let Ok(content) = get_clipboard_wl_paste().await {
            // Simple hash to detect changes without storing full content.
            let hash = simple_hash(&content);
            if hash != last_hash && !content.is_empty() {
                let preview = truncate(&content, PREVIEW_MAX);
                let content_type = detect_content_type(&content);

                info!("Clipboard updated: {content_type} — {preview:?}");

                bus.publish(SystemEvent::new(
                    "clipboard-watcher",
                    EventKind::Clipboard,
                    EventPayload::ClipboardUpdated {
                        content_type,
                        preview,
                    },
                ));

                last_hash = hash;
            }
        }
    }
}

async fn get_clipboard_wl_paste() -> Result<String, ()> {
    let output = tokio::process::Command::new("wl-paste")
        .arg("--no-newline")
        .output()
        .await
        .map_err(|_| ())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string())
    } else {
        Err(())
    }
}

// ── xclip (polling) ──────────────────────────────────────────────

async fn run_xclip_poll(bus: Arc<EventBus>) {
    info!("Clipboard watcher: using xclip polling");

    let mut last_hash = String::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        if let Ok(content) = get_clipboard_xclip().await {
            let hash = simple_hash(&content);
            if hash != last_hash && !content.is_empty() {
                let preview = truncate(&content, PREVIEW_MAX);
                let content_type = detect_content_type(&content);

                info!("Clipboard updated: {content_type} — {preview:?}");

                bus.publish(SystemEvent::new(
                    "clipboard-watcher",
                    EventKind::Clipboard,
                    EventPayload::ClipboardUpdated {
                        content_type,
                        preview,
                    },
                ));

                last_hash = hash;
            }
        }
    }
}

async fn get_clipboard_xclip() -> Result<String, ()> {
    let output = tokio::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .await
        .map_err(|_| ())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string())
    } else {
        Err(())
    }
}

// ── Utilities ────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn detect_content_type(content: &str) -> String {
    if content.starts_with("http://") || content.starts_with("https://") {
        "url".to_string()
    } else if content.contains('\n') && content.lines().count() > 3 {
        "multiline-text".to_string()
    } else if content.starts_with('/') || content.contains('/') {
        "path".to_string()
    } else {
        "text".to_string()
    }
}

fn simple_hash(s: &str) -> String {
    // Simple FNV-1a hash for change detection.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:x}")
}
