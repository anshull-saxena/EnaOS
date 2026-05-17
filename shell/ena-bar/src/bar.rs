use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::glib;
use serde_json::Value;

use crate::ipc::EnadEvent;

/// Internal state of the Ena Bar.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BarState {
    /// Collapsed — pill showing status dot.
    Collapsed,
    /// Expanded — input field visible, ready for command.
    Expanded,
    /// Thinking — enad processing, animated indicator shown.
    Thinking,
    /// Result — display command output.
    Result { text: String },
}

/// System context state — tracks real OS awareness.
#[derive(Debug, Clone, Default)]
struct SystemContext {
    focused_app: String,
    focused_title: String,
    workspace: String,
    battery_pct: f64,
    battery_state: String,
    network_connected: bool,
    network_ssid: String,
    audio_volume: f64,
    audio_muted: bool,
    media_player: String,
    media_state: String,
    media_title: String,
}

/// The Ena Bar widget tree.
pub struct EnaBar {
    pub container: gtk4::Box,
    pub(crate) state: std::sync::Mutex<BarState>,

    pub(crate) status_dot: gtk4::DrawingArea,
    input_entry: gtk4::Entry,
    mic_button: gtk4::Button,
    spinner: gtk4::Spinner,
    result_label: gtk4::Label,
    result_revealer: gtk4::Revealer,
    status_label: gtk4::Label,
    status_revealer: gtk4::Revealer,

    // System awareness widgets.
    context_label: gtk4::Label,
    context_revealer: gtk4::Revealer,

    // Action execution display.
    action_label: gtk4::Label,
    action_revealer: gtk4::Revealer,

    // Internal context tracker.
    context: std::sync::Mutex<SystemContext>,
}

impl EnaBar {
    /// Build the complete Ena Bar widget tree.
    pub fn new() -> Arc<Self> {
        // ── Status dot with frame clock animation ───────────────
        let status_dot = gtk4::DrawingArea::new();
        status_dot.set_content_width(12);
        status_dot.set_content_height(12);
        status_dot.set_halign(gtk4::Align::Center);
        status_dot.set_valign(gtk4::Align::Center);

        status_dot.add_tick_callback(move |widget, _frame_clock| {
            widget.queue_draw();
            glib::ControlFlow::Continue
        });

        status_dot.set_draw_func(move |_area, cr, width, height| {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let cx = width as f64 / 2.0;
            let cy = height as f64 / 2.0;
            let radius = (width.min(height) as f64 / 2.0) - 1.0;
            cr.arc(cx, cy, radius, 0.0, 2.0 * std::f64::consts::PI);
            let breathe = (t * 1.5).sin() * 0.15 + 0.85;
            cr.set_source_rgba(0.5, 0.5, 0.5, breathe.max(0.6));
            let _ = cr.fill();
        });

        // ── Input entry ─────────────────────────────────────────
        let input_entry = gtk4::Entry::builder()
            .placeholder_text("Ask Ena...")
            .hexpand(true)
            .height_request(44)
            .build();
        input_entry.add_css_class("ena-input");

        // ── Mic button ──────────────────────────────────────────
        let mic_button = gtk4::Button::builder()
            .icon_name("audio-input-microphone-symbolic")
            .tooltip_text("Voice input")
            .build();
        mic_button.add_css_class("ena-mic-button");

        // ── Spinner ─────────────────────────────────────────────
        let spinner = gtk4::Spinner::builder()
            .width_request(20)
            .height_request(20)
            .build();

        // ── Result label ────────────────────────────────────────
        let result_label = gtk4::Label::builder()
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::WordChar)
            .xalign(0.0)
            .yalign(0.0)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();
        result_label.add_css_class("ena-result");

        let result_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .child(&result_label)
            .build();

        // ── Status bar ──────────────────────────────────────────
        let status_label = gtk4::Label::builder()
            .label("enad: disconnected")
            .xalign(0.0)
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(2)
            .build();
        status_label.add_css_class("ena-status");

        let status_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideUp)
            .transition_duration(200)
            .child(&status_label)
            .build();

        // ── Context label (system awareness) ────────────────────
        let context_label = gtk4::Label::builder()
            .label("")
            .xalign(1.0)
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(2)
            .build();
        context_label.add_css_class("ena-context");

        let context_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideUp)
            .transition_duration(200)
            .child(&context_label)
            .build();

        // ── Action execution label ──────────────────────────────
        let action_label = gtk4::Label::builder()
            .label("")
            .xalign(0.5)
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(2)
            .build();
        action_label.add_css_class("ena-action");

        let action_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .child(&action_label)
            .build();

        // ── Main bar row ────────────────────────────────────────
        let bar_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .height_request(56)
            .css_classes(["ena-bar-row"])
            .build();
        bar_row.append(&status_dot);
        bar_row.append(&spinner);
        bar_row.append(&input_entry);
        bar_row.append(&mic_button);

        // ── Root container ──────────────────────────────────────
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .css_classes(["ena-bar-container"])
            .build();
        container.append(&result_revealer);
        container.append(&bar_row);
        container.append(&status_revealer);
        container.append(&context_revealer);
        container.append(&action_revealer);

        let bar = Arc::new(EnaBar {
            container,
            state: std::sync::Mutex::new(BarState::Collapsed),
            status_dot,
            input_entry,
            mic_button,
            spinner,
            result_label,
            result_revealer,
            status_label,
            status_revealer,
            context_label,
            context_revealer,
            action_label,
            action_revealer,
            context: std::sync::Mutex::new(SystemContext::default()),
        });

        // Wire up input entry activation (Enter key)
        let b = bar.clone();
        bar.input_entry.connect_activate(move |entry| {
            let text = entry.text().to_string();
            if !text.is_empty() {
                b.set_state(BarState::Thinking);
                tracing::info!("Command submitted: {text}");
            }
        });

        // Wire up mic button
        let b = bar.clone();
        bar.mic_button.connect_clicked(move |_| {
            tracing::info!("Mic button clicked");
            b.show_status("Listening...", 2);
        });

        bar
    }

    /// Update the bar's visual state. Must be called from the GTK main thread.
    pub(crate) fn set_state(&self, state: BarState) {
        let mut current = self.state.lock().unwrap();
        *current = state.clone();
        drop(current);

        match state {
            BarState::Collapsed => {
                self.input_entry.set_visible(false);
                self.spinner.stop();
                self.spinner.set_visible(false);
                self.mic_button.set_visible(false);
                self.result_revealer.set_reveal_child(false);
                self.status_dot.set_visible(true);
                self.container.set_size_request(-1, 48);
            }
            BarState::Expanded => {
                self.input_entry.set_visible(true);
                self.spinner.stop();
                self.spinner.set_visible(false);
                self.mic_button.set_visible(true);
                self.result_revealer.set_reveal_child(false);
                self.status_dot.set_visible(true);
                self.container.set_size_request(-1, 56);
                self.input_entry.grab_focus();
            }
            BarState::Thinking => {
                self.input_entry.set_visible(true);
                self.spinner.start();
                self.spinner.set_visible(true);
                self.mic_button.set_visible(false);
                self.result_revealer.set_reveal_child(false);
                self.status_dot.set_visible(true);
                self.container.set_size_request(-1, 56);
            }
            BarState::Result { ref text } => {
                self.result_label.set_label(text);
                self.result_revealer.set_reveal_child(true);
                self.spinner.stop();
                self.spinner.set_visible(false);
                self.input_entry.set_visible(true);
                self.mic_button.set_visible(true);
                self.status_dot.set_visible(true);
            }
        }
    }

    /// Update the context label with current system awareness.
    fn update_context(&self) {
        let ctx = self.context.lock().unwrap();
        let mut parts = Vec::new();

        if !ctx.focused_app.is_empty() {
            parts.push(format!("Focused: {}", ctx.focused_app));
        }
        if !ctx.workspace.is_empty() {
            parts.push(ctx.workspace.clone());
        }
        if !ctx.battery_state.is_empty()
            && ctx.battery_state != "fully-charged"
            && ctx.battery_state != "unknown"
        {
            let icon = match ctx.battery_state.as_str() {
                "charging" => "\u{26A1}",
                "discharging" if ctx.battery_pct < 20.0 => "\u{1F534}",
                "discharging" if ctx.battery_pct < 50.0 => "\u{1F7E1}",
                "discharging" => "\u{1F7E2}",
                _ => "",
            };
            parts.push(format!("{}{:.0}%", icon, ctx.battery_pct));
        }
        if !ctx.network_connected {
            parts.push("Network: disconnected".to_string());
        } else if !ctx.network_ssid.is_empty() {
            parts.push(format!("WiFi: {}", ctx.network_ssid));
        }
        if !ctx.media_player.is_empty() && ctx.media_state == "Playing" {
            let display = if !ctx.media_title.is_empty() {
                format!("{}: {}", ctx.media_player, ctx.media_title)
            } else {
                ctx.media_player.clone()
            };
            parts.push(format!("Playing: {}", display));
        }

        let text = parts.join("  |  ");
        if !text.is_empty() {
            self.context_label.set_label(&text);
            self.context_revealer.set_reveal_child(true);
        } else {
            self.context_revealer.set_reveal_child(false);
        }
    }

    /// Handle an IPC event from enad. Must be called from the GTK main thread.
    pub fn handle_event(&self, event: EnadEvent) {
        match event {
            EnadEvent::Connected => {
                self.status_label.set_label("enad: connected");
                self.status_revealer.set_reveal_child(true);
                let status_rev = self.status_revealer.clone();
                glib::timeout_add_seconds_local(2, move || {
                    status_rev.set_reveal_child(false);
                    glib::ControlFlow::Break
                });
                self.update_status_dot(0.2, 0.8, 0.3);
                self.set_state(BarState::Expanded);
            }
            EnadEvent::Disconnected => {
                self.status_label
                    .set_label("enad: disconnected \u{2014} reconnecting...");
                self.status_revealer.set_reveal_child(true);
                self.update_status_dot(0.5, 0.5, 0.5);
                self.set_state(BarState::Collapsed);
            }
            EnadEvent::Pong { latency_ms } => {
                let label = format!("enad: connected ({latency_ms}ms)");
                self.status_label.set_label(&label);
                self.update_status_dot(0.2, 0.8, 0.3);
                self.set_state(BarState::Expanded);
            }
            EnadEvent::SystemEvent { kind, payload } => {
                tracing::info!("System event: {kind}: {payload}");

                // Update system context from daemon events.
                self.update_context_from_event(&kind, &payload);

                // Handle command/agent result events.
                match kind.as_str() {
                    "agent_result" | "command_result" => {
                        let text = payload
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("No output");
                        self.set_state(BarState::Result {
                            text: text.to_string(),
                        });
                    }
                    "agent_thinking" | "command_start" => {
                        self.set_state(BarState::Thinking);
                    }
                    "agent_error" | "command_error" => {
                        let text = payload
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("An error occurred");
                        self.set_state(BarState::Result {
                            text: format!("Error: {text}"),
                        });
                    }
                    "system_info" => {
                        if let Some(hostname) =
                            payload.get("hostname").and_then(|v| v.as_str())
                        {
                            self.status_label
                                .set_label(&format!("enad: {hostname}"));
                        }
                    }
                    _ => {}
                }

                // Handle action lifecycle events.
                self.handle_action_event(&kind, &payload);
            }
            EnadEvent::Raw(raw) => {
                tracing::warn!("Unparsed IPC message: {raw}");
            }
        }
    }

    /// Update internal system context from a daemon event.
    fn update_context_from_event(&self, kind: &str, payload: &Value) {
        let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let data = payload.get("data").cloned().unwrap_or(Value::Null);

        let mut ctx = self.context.lock().unwrap();

        match kind {
            "Window" => match event_type {
                "WindowFocused" => {
                    if let Some(app) = data.get("app").and_then(|v| v.as_str()) {
                        ctx.focused_app = app.to_string();
                    }
                    if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                        ctx.focused_title = title.to_string();
                    }
                }
                "WorkspaceChanged" => {
                    if let Some(ws) = data.get("workspace").and_then(|v| v.as_str()) {
                        ctx.workspace = ws.to_string();
                    }
                }
                _ => {}
            },
            "System" => match event_type {
                "BatteryStatus" => {
                    if let Some(pct) = data.get("percentage").and_then(|v| v.as_f64()) {
                        ctx.battery_pct = pct;
                    }
                    if let Some(state) = data.get("state").and_then(|v| v.as_str()) {
                        ctx.battery_state = state.to_string();
                    }
                }
                "NetworkStatus" => {
                    if let Some(connected) = data.get("connected").and_then(|v| v.as_bool()) {
                        ctx.network_connected = connected;
                    }
                    if let Some(ssid) = data.get("ssid").and_then(|v| v.as_str()) {
                        ctx.network_ssid = ssid.to_string();
                    }
                }
                _ => {}
            },
            "Audio" => match event_type {
                "AudioVolumeChanged" => {
                    if let Some(vol) = data.get("volume").and_then(|v| v.as_f64()) {
                        ctx.audio_volume = vol;
                    }
                    if let Some(muted) = data.get("muted").and_then(|v| v.as_bool()) {
                        ctx.audio_muted = muted;
                    }
                }
                "MediaPlayback" => {
                    if let Some(player) = data.get("player").and_then(|v| v.as_str()) {
                        ctx.media_player = player.to_string();
                    }
                    if let Some(state) = data.get("state").and_then(|v| v.as_str()) {
                        ctx.media_state = state.to_string();
                    }
                    if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                        ctx.media_title = title.to_string();
                    }
                }
                _ => {}
            },
            _ => {}
        }

        drop(ctx);
        self.update_context();
    }

    /// Handle action lifecycle events and display execution state.
    fn handle_action_event(&self, kind: &str, payload: &Value) {
        if kind != "System" {
            return;
        }

        let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let data = payload.get("data").cloned().unwrap_or(Value::Null);

        match event_type {
            "ActionStarted" => {
                let message = data
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Executing action...");
                self.action_label.set_label(message);
                self.action_revealer.set_reveal_child(true);
                self.update_status_dot(0.85, 0.7, 0.2);
            }
            "ActionCompleted" => {
                let result = data
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Action completed");
                self.action_label.set_label(&format!("\u{2713} {result}"));
                self.update_status_dot(0.2, 0.8, 0.3);

                // Auto-hide after 3 seconds.
                let action_rev = self.action_revealer.clone();
                glib::timeout_add_seconds_local(3, move || {
                    action_rev.set_reveal_child(false);
                    glib::ControlFlow::Break
                });
            }
            "ActionFailed" => {
                let error = data
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Action failed");
                self.action_label.set_label(&format!("\u{2717} {error}"));
                self.update_status_dot(0.8, 0.3, 0.3);

                let action_rev = self.action_revealer.clone();
                glib::timeout_add_seconds_local(5, move || {
                    action_rev.set_reveal_child(false);
                    glib::ControlFlow::Break
                });
            }
            "ActionCancelled" => {
                self.action_label.set_label("Action cancelled");
                self.update_status_dot(0.5, 0.5, 0.5);

                let action_rev = self.action_revealer.clone();
                glib::timeout_add_seconds_local(2, move || {
                    action_rev.set_reveal_child(false);
                    glib::ControlFlow::Break
                });
            }
            _ => {}
        }
    }

    pub(crate) fn update_status_dot(&self, r: f64, g: f64, b: f64) {
        let dot = &self.status_dot;
        dot.set_draw_func(move |_area, cr, width, height| {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let cx = width as f64 / 2.0;
            let cy = height as f64 / 2.0;
            let radius = (width.min(height) as f64 / 2.0) - 1.0;
            cr.arc(cx, cy, radius, 0.0, 2.0 * std::f64::consts::PI);
            let breathe = (t * 1.5).sin() * 0.15 + 0.85;
            cr.set_source_rgba(r, g, b, breathe.max(0.6));
            let _ = cr.fill();
        });
        dot.queue_draw();
    }

    /// Show a transient status message. Must be called from the GTK main thread.
    pub fn show_status(&self, message: &str, duration_secs: u32) {
        self.status_label.set_label(message);
        self.status_revealer.set_reveal_child(true);
        let status_rev = self.status_revealer.clone();
        glib::timeout_add_seconds_local(duration_secs, move || {
            status_rev.set_reveal_child(false);
            glib::ControlFlow::Break
        });
    }
}
