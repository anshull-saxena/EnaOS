use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use gtk4::prelude::*;
use serde_json::Value;

use crate::ipc;

/// A single action in the restore preview list.
#[derive(Debug, Clone)]
struct PreviewAction {
    id: String,
    action_type: String,
    label: String,
    safe: bool,
    selected: bool,
}

/// Compact snapshot summary shown in the suggestion bar.
#[derive(Debug, Clone, Default)]
struct SnapshotSummary {
    id: String,
    label: String,
    created_at: String,
    window_count: i64,
    terminal_count: i64,
    active_project: String,
}

/// Internal state of the restoration suggestion widget.
#[derive(Debug, Clone)]
enum RestorationState {
    Hidden,
    Suggesting(SnapshotSummary),
    Preview {
        summary: SnapshotSummary,
        actions: Vec<PreviewAction>,
    },
    Restoring,
}

/// Relative time label from an ISO 8601 timestamp.
fn relative_time(iso: &str) -> String {
    // Parse: "2025-06-15T10:30:00Z" or "2025-06-15T10:30:00.123456Z"
    if iso.len() < 20 {
        return String::new();
    }
    let s = iso.trim();
    let date_part = &s[..10];
    let rest = &s[11..];
    let time_part = rest.split('Z').next().unwrap_or(rest);
    let time_part = time_part.split('+').next().unwrap_or(time_part);
    // Handle timezone offset: "-05:00" after the time
    // Just take the first part before any + or - (but after T)
    // Format: HH:MM:SS or HH:MM:SS.xxxxxx
    let time_clean = if let Some(idx) = time_part.rfind(['+', '-']) {
        if idx > 8 {
            // It's a timezone offset, not part of a date
            &time_part[..idx]
        } else {
            time_part
        }
    } else {
        time_part
    };

    let year: i64 = match date_part[0..4].parse() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let month: u32 = match date_part[5..7].parse() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let day: u32 = match date_part[8..10].parse() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    // Extract hour:minute:second
    let parts: Vec<&str> = time_clean.split(':').collect();
    if parts.len() < 2 {
        return String::new();
    }
    let hour: u32 = match parts[0].parse() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let minute: u32 = match parts[1].parse() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let second: u32 = if parts.len() > 2 {
        parts[2]
            .split('.')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0)
    } else {
        0
    };

    // Compute unix timestamp.
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Days from civil date (Howard Hinnant algorithm).
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy =
        (153 * if month > 2 {
            month as i64 - 3
        } else {
            month as i64 + 9
        } + 2)
            / 5
            + day as i64
            - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let then = days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;

    let diff = now.saturating_sub(then);
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Parse a snapshot JSON value into SnapshotSummary.
fn parse_snapshot(snap: &Value) -> SnapshotSummary {
    SnapshotSummary {
        id: snap
            .get("snapshot_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        label: snap
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("Workspace")
            .to_string(),
        created_at: snap
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        window_count: snap.get("app_count").and_then(|v| v.as_i64()).unwrap_or(0),
        terminal_count: snap
            .get("terminal_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        active_project: String::new(),
    }
}

/// Parse a preview response JSON into actions.
fn parse_preview_actions(data: &Value) -> Vec<PreviewAction> {
    data.get("actions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|a| PreviewAction {
                    id: format!(
                        "{}-{}",
                        a.get("action_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("action"),
                        a.get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                    ),
                    action_type: a
                        .get("action_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Action")
                        .to_string(),
                    label: a
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    safe: !a
                        .get("requires_approval")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    selected: true,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse IPC response body from enad, navigating through the envelope.
///
/// Enad response wire format:
///   {"id": "...", "kind": {"type": "Response", "body": {"Data": {"payload": ...}}}}
///                                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
/// We extract the inner payload from the Response enum (which uses default serde
/// representation, so variant name "Data" is a key in the body object).
fn get_response_body(response: &Value) -> Option<&Value> {
    let kind = response.get("kind")?;
    let body = kind.get("body")?;
    body.get("Ok").or_else(|| body.get("Data")).or(Some(body))
}

fn extract_snapshots(response: &Value) -> Vec<Value> {
    let body = match get_response_body(response) {
        Some(b) => b,
        None => return Vec::new(),
    };
    // enad wraps data in "payload": {"snapshots": [...]} for Data variant.
    let payload = body.get("payload");
    if let Some(arr) = payload.and_then(|p| p.get("snapshots").and_then(|v| v.as_array())) {
        return arr.clone();
    }
    // Try direct array (fallback).
    if let Some(arr) = body.as_array() {
        return arr.clone();
    }
    // Try direct array under payload.
    if let Some(arr) = payload.and_then(|p| p.as_array()) {
        return arr.clone();
    }
    Vec::new()
}

fn extract_preview(response: &Value) -> Option<Vec<PreviewAction>> {
    let body = get_response_body(response)?;
    // enad wraps data in "payload" for Data variant.
    let payload = body.get("payload").unwrap_or(body);
    let actions = parse_preview_actions(payload);
    if actions.is_empty() {
        // Try nested in "preview".
        payload.get("preview").map(parse_preview_actions)
    } else {
        Some(actions)
    }
}

/// Restoration suggestion widget.
///
/// Shows a compact suggestion when recent snapshots are available:
/// "Continue: EnaOS Development · 2h ago" with a [Restore] button.
/// On click, expands to show a preview with per-action toggles.
pub struct RestorationWidget {
    pub container: gtk4::Revealer,
    revealer: gtk4::Revealer,

    // Compact suggestion bar.
    suggestion_button: gtk4::Button,
    suggestion_label: gtk4::Label,
    suggestion_time: gtk4::Label,
    suggestion_icon: gtk4::Label,

    // Preview pane.
    preview_revealer: gtk4::Revealer,
    preview_title: gtk4::Label,
    action_list: gtk4::ListBox,
    restore_button: gtk4::Button,
    cancel_button: gtk4::Button,

    // State.
    state: Mutex<RestorationState>,
    socket_path: Mutex<String>,

    /// Channel sender for IPC responses from background threads.
    pub cmd_tx: mpsc::Sender<Value>,
    cmd_rx: Mutex<mpsc::Receiver<Value>>,

    // Callbacks (set by bar.rs). Called from GTK main thread only.
    pub on_restore: Mutex<Option<Box<dyn Fn(String)>>>,
    pub on_dismiss: Mutex<Option<Box<dyn Fn()>>>,
}

impl RestorationWidget {
    pub fn new(socket_path: String) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel();

        // ── Suggestion button (compact bar) ─────────────────────
        let suggestion_icon = gtk4::Label::builder()
            .label("↶")
            .width_request(20)
            .xalign(0.5)
            .build();
        suggestion_icon.add_css_class("ena-restore-icon");

        let suggestion_label = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        suggestion_label.add_css_class("ena-restore-suggestion-label");

        let suggestion_time = gtk4::Label::builder().xalign(1.0).build();
        suggestion_time.add_css_class("ena-restore-time");

        let suggestion_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        suggestion_box.append(&suggestion_icon);
        suggestion_box.append(&suggestion_label);
        suggestion_box.append(&suggestion_time);

        let suggestion_button = gtk4::Button::builder()
            .child(&suggestion_box)
            .css_classes(["ena-restore-suggestion"])
            .build();

        // ── Preview pane ────────────────────────────────────────
        let preview_title = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .wrap(true)
            .build();
        preview_title.add_css_class("ena-restore-preview-title");

        let action_list = gtk4::ListBox::builder()
            .activate_on_single_click(false)
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        action_list.add_css_class("ena-restore-action-list");

        let restore_button = gtk4::Button::with_label("Restore Workspace");
        restore_button.add_css_class("ena-restore-confirm-btn");

        let cancel_button = gtk4::Button::with_label("Dismiss");
        cancel_button.add_css_class("ena-restore-dismiss-btn");

        let preview_button_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::End)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        preview_button_box.append(&cancel_button);
        preview_button_box.append(&restore_button);

        let preview_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .build();
        preview_box.append(&preview_title);
        preview_box.append(&action_list);
        preview_box.append(&preview_button_box);

        let preview_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .child(&preview_box)
            .build();

        // ── Root container ──────────────────────────────────────
        let root = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .css_classes(["ena-restore-container"])
            .build();
        root.append(&suggestion_button);
        root.append(&preview_revealer);

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(250)
            .child(&root)
            .build();

        // Clone widgets before moving into struct for signal wiring.
        let suggest_btn = suggestion_button.clone();
        let restore_btn = restore_button.clone();
        let cancel_btn = cancel_button.clone();
        let container_rev = revealer.clone();

        let widget = Arc::new(RestorationWidget {
            container: container_rev,
            revealer,
            suggestion_button,
            suggestion_label,
            suggestion_time,
            suggestion_icon,
            preview_revealer,
            preview_title,
            action_list,
            restore_button,
            cancel_button,
            state: Mutex::new(RestorationState::Hidden),
            socket_path: Mutex::new(socket_path),
            cmd_tx,
            cmd_rx: Mutex::new(cmd_rx),
            on_restore: Mutex::new(None),
            on_dismiss: Mutex::new(None),
        });

        // Wire suggestion button click → show preview.
        let w = widget.clone();
        suggest_btn.connect_clicked(move |_| {
            let state = w.state.lock().unwrap().clone();
            if let RestorationState::Suggesting(summary) = &state {
                let sid = summary.id.clone();
                drop(state);
                w.fetch_preview(&sid);
            }
        });

        // Wire restore button.
        let w = widget.clone();
        restore_btn.connect_clicked(move |_| {
            let state = w.state.lock().unwrap().clone();
            let snapshot_id = match &state {
                RestorationState::Preview { summary, .. } => summary.id.clone(),
                _ => return,
            };
            drop(state);
            w.trigger_restore(&snapshot_id);
        });

        // Wire cancel/dismiss button.
        let w = widget.clone();
        cancel_btn.connect_clicked(move |_| {
            w.dismiss();
        });

        widget
    }

    /// Poll for IPC command responses. Call from GTK idle handler.
    pub fn poll(&self) {
        let rx = self.cmd_rx.lock().unwrap();
        while let Ok(response) = rx.try_recv() {
            self.handle_response(response);
        }
    }

    /// Handle a response Value (called on GTK main thread).
    fn handle_response(&self, response: Value) {
        let command = response
            .get("_command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match command {
            "ListSnapshots" => {
                let snapshots = extract_snapshots(&response);
                if let Some(snap) = snapshots.into_iter().next() {
                    let summary = parse_snapshot(&snap);
                    self.show_suggestion(summary);
                }
            }
            "PreviewRestore" => {
                if let Some(actions) = extract_preview(&response) {
                    self.show_preview(actions);
                } else {
                    self.preview_title
                        .set_label("Failed to load preview — no actions available");
                    self.restore_button.set_sensitive(true);
                }
            }
            "RestoreSnapshot" => {
                // Check for error — navigate through the enad envelope.
                let kind = response.get("kind");
                let body = kind.and_then(|k| k.get("body"));
                let is_error = body
                    .and_then(|b| b.get("Error"))
                    .or_else(|| body.and_then(|b| b.get("error")))
                    .is_some();
                if is_error {
                    self.show_error("Restore failed");
                }
                // If no error, orchestration events will flow via the event stream.
            }
            _ => {}
        }
    }

    /// Fetch the most recent snapshot (runs in background thread).
    pub fn check_for_snapshots(&self) {
        let path = self.socket_path.lock().unwrap().clone();
        let tx = self.cmd_tx.clone();
        std::thread::spawn(move || {
            let result =
                ipc::send_command(&path, "ListSnapshots", &serde_json::json!({"limit": 1}));
            match result {
                Ok(mut response) => {
                    response["_command"] = serde_json::json!("ListSnapshots");
                    let _ = tx.send(response);
                }
                Err(e) => {
                    tracing::warn!("Failed to list snapshots: {e}");
                }
            }
        });
    }

    /// Fetch restore preview from enad (runs in background thread).
    fn fetch_preview(&self, snapshot_id: &str) {
        let path = self.socket_path.lock().unwrap().clone();
        let sid = snapshot_id.to_string();
        let tx = self.cmd_tx.clone();

        // Show loading state immediately.
        self.preview_title.set_label("Loading preview…");
        self.action_list.set_visible(false);
        self.restore_button.set_sensitive(false);
        self.preview_revealer.set_reveal_child(true);

        std::thread::spawn(move || {
            let result = ipc::send_command(
                &path,
                "PreviewRestore",
                &serde_json::json!({"snapshot_id": sid}),
            );
            match result {
                Ok(mut response) => {
                    response["_command"] = serde_json::json!("PreviewRestore");
                    let _ = tx.send(response);
                }
                Err(e) => {
                    tracing::warn!("Failed to get preview: {e}");
                }
            }
        });
    }

    /// Show the compact suggestion bar.
    fn show_suggestion(&self, summary: SnapshotSummary) {
        let project = if !summary.active_project.is_empty() {
            summary.active_project.clone()
        } else {
            summary.label.clone()
        };
        let time = relative_time(&summary.created_at);

        self.suggestion_label.set_label(&format!(
            "Continue: {}  ·  {} windows, {} terminals",
            project, summary.window_count, summary.terminal_count,
        ));
        self.suggestion_time.set_label(&time);
        *self.state.lock().unwrap() = RestorationState::Suggesting(summary);
        self.revealer.set_reveal_child(true);
    }

    /// Show the expanded preview pane with action list.
    fn show_preview(&self, actions: Vec<PreviewAction>) {
        let state = self.state.lock().unwrap().clone();
        let summary = match &state {
            RestorationState::Suggesting(s) => s.clone(),
            _ => return,
        };

        let project = if !summary.active_project.is_empty() {
            summary.active_project.clone()
        } else {
            summary.label.clone()
        };

        self.preview_title.set_label(&format!(
            "Restore: {}  ·  {} actions",
            project,
            actions.len(),
        ));

        // Clear existing action rows.
        while let Some(child) = self.action_list.first_child() {
            self.action_list.remove(&child);
        }

        for action in &actions {
            let row = self.build_action_row(action);
            self.action_list.append(&row);
        }

        self.action_list.set_visible(true);
        self.restore_button.set_sensitive(true);

        *self.state.lock().unwrap() = RestorationState::Preview { summary, actions };
    }

    /// Build a single action row with toggle.
    fn build_action_row(&self, action: &PreviewAction) -> gtk4::Box {
        let check = gtk4::CheckButton::builder().active(action.selected).build();

        let type_label = gtk4::Label::builder()
            .label(&action.action_type)
            .width_request(80)
            .xalign(0.0)
            .build();
        type_label.add_css_class("ena-restore-action-type");

        let action_label = gtk4::Label::builder()
            .label(&action.label)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        action_label.add_css_class("ena-restore-action-label");

        let badge_label = gtk4::Label::builder()
            .label(if action.safe { "" } else { "requires approval" })
            .build();
        if !action.safe {
            badge_label.add_css_class("ena-restore-badge-risky");
        }

        let row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(2)
            .margin_bottom(2)
            .build();
        row.append(&check);
        row.append(&type_label);
        row.append(&action_label);
        row.append(&badge_label);

        row
    }

    /// Collect selected action IDs from the preview checkboxes.
    fn collect_selected_ids(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        let actions = match &*state {
            RestorationState::Preview { actions, .. } => actions.clone(),
            _ => return Vec::new(),
        };
        drop(state);

        let mut ids = Vec::new();
        let mut child = self.action_list.first_child();
        let mut idx = 0usize;
        while let Some(row) = child {
            // Use reference-based downcast to avoid moving `row`.
            if let Some(box_row) = row.downcast_ref::<gtk4::Box>()
                && let Some(check) = box_row
                    .first_child()
                    .and_then(|c| c.downcast::<gtk4::CheckButton>().ok())
                && check.is_active()
                && let Some(action) = actions.get(idx)
            {
                ids.push(action.id.clone());
            }
            child = row.next_sibling();
            idx += 1;
        }
        ids
    }

    /// Trigger restore for the given snapshot.
    fn trigger_restore(&self, snapshot_id: &str) {
        let selected_ids = self.collect_selected_ids();

        let path = self.socket_path.lock().unwrap().clone();
        let sid = snapshot_id.to_string();
        let tx = self.cmd_tx.clone();

        // Update state.
        *self.state.lock().unwrap() = RestorationState::Restoring;

        // Disable restore button.
        self.restore_button.set_sensitive(false);
        self.restore_button.set_label("Restoring…");

        // Notify bar.
        if let Some(ref cb) = *self.on_restore.lock().unwrap() {
            cb(sid.clone());
        }

        std::thread::spawn(move || {
            let selections = if selected_ids.is_empty() {
                None
            } else {
                Some(serde_json::json!({"action_ids": selected_ids}))
            };

            let body = if let Some(sel) = selections {
                serde_json::json!({"snapshot_id": sid, "selections": sel})
            } else {
                serde_json::json!({"snapshot_id": sid})
            };

            let result = ipc::send_command(&path, "RestoreSnapshot", &body);
            match result {
                Ok(mut response) => {
                    response["_command"] = serde_json::json!("RestoreSnapshot");
                    let _ = tx.send(response);
                }
                Err(e) => {
                    tracing::error!("Restore failed: {e}");
                    // Send error response.
                    let err = serde_json::json!({
                        "_command": "RestoreSnapshot",
                        "body": {"Error": {"message": e}}
                    });
                    let _ = tx.send(err);
                }
            }
        });
    }

    /// Show an error state.
    fn show_error(&self, message: &str) {
        self.preview_title.set_label(message);
        self.preview_revealer.set_reveal_child(true);
        self.restore_button.set_label("Retry");
        self.restore_button.set_sensitive(true);
        *self.state.lock().unwrap() = RestorationState::Hidden;
    }

    /// Dismiss the restoration suggestion.
    pub fn dismiss(&self) {
        self.revealer.set_reveal_child(false);
        self.preview_revealer.set_reveal_child(false);
        *self.state.lock().unwrap() = RestorationState::Hidden;
        if let Some(ref cb) = *self.on_dismiss.lock().unwrap() {
            cb();
        }
    }

    /// Called when orchestration events show restore has started.
    pub fn on_restore_started(&self) {
        self.preview_revealer.set_reveal_child(false);
        self.suggestion_label.set_label("Restoring workspace…");
        self.suggestion_icon.set_label("⟳");
    }

    /// Called when restore completes (from orchestration events).
    pub fn on_restore_completed(&self) {
        self.suggestion_label.set_label("✓ Workspace restored");
        self.suggestion_icon.set_label("✓");
        // Auto-dismiss is handled in bar.rs via the orchestration event handler,
        // which calls this method and then schedules dismiss via timeout.
        // No direct timeout here — bar.rs handles it.
    }

    /// Called when restore fails.
    pub fn on_restore_failed(&self) {
        self.restore_button.set_label("Retry");
        self.restore_button.set_sensitive(true);
        self.preview_revealer.set_reveal_child(true);
        *self.state.lock().unwrap() = RestorationState::Hidden;
    }

    /// Check if suggestion is visible.
    pub fn is_visible(&self) -> bool {
        self.revealer.reveals_child()
    }
}
