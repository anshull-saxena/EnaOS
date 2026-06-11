use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;

use crate::ambient_ui::{AmbientSuggestionWidget, parse_suggestion_event};
use crate::command_palette::CommandPalette;
use crate::ipc::EnadEvent;
use crate::orchestration_ui::{
    OrchestrationDisplay, TimelineWidget, parse_node_event, parse_plan_event,
};
use crate::restoration_ui::RestorationWidget;
use crate::welcome_overlay::WelcomeOverlay;

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

    // Orchestration execution visibility.
    timeline: Arc<TimelineWidget>,

    // Restoration suggestion widget.
    restoration: Arc<RestorationWidget>,

    // Ambient suggestion widget.
    ambient: Arc<AmbientSuggestionWidget>,

    // Contextual command palette.
    command_palette: Arc<CommandPalette>,

    // Welcome overlay for first-run onboarding.
    welcome_overlay: Arc<WelcomeOverlay>,

    // Whether we triggered a restore and are waiting for it.
    is_restoring: AtomicBool,
    // Whether first-run status has been checked this session.
    first_run_checked: AtomicBool,
    // Socket path for IPC commands.
    socket_path: String,

    // Internal context tracker.
    context: std::sync::Mutex<SystemContext>,
}

impl EnaBar {
    /// Build the complete Ena Bar widget tree.
    pub fn new(socket_path: &str) -> Arc<Self> {
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

        // ── Orchestration timeline ──────────────────────────────
        let timeline = TimelineWidget::new();

        // ── Restoration suggestion widget ───────────────────────
        let restoration = RestorationWidget::new(socket_path.to_string());

        // ── Ambient suggestion widget ───────────────────────────
        let ambient = AmbientSuggestionWidget::new();

        // ── Contextual command palette ──────────────────────────
        let command_palette = CommandPalette::new();

        // ── Welcome overlay (first-run onboarding) ──────────────
        let welcome_overlay = Arc::new(WelcomeOverlay::new());

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
        container.append(&welcome_overlay.container);
        container.append(&bar_row);
        container.append(&command_palette.container);
        container.append(&restoration.container);
        container.append(&ambient.container);
        container.append(&timeline.container);
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
            timeline,
            restoration,
            ambient,
            command_palette: Arc::new(command_palette),
            welcome_overlay,
            is_restoring: AtomicBool::new(false),
            first_run_checked: AtomicBool::new(false),
            socket_path: socket_path.to_string(),
            context: std::sync::Mutex::new(SystemContext::default()),
        });

        // Wire palette selection → action execution.
        let exec_socket = socket_path.to_string();
        bar.command_palette.set_on_select(move |suggestion| {
            tracing::info!(
                "Command palette selected: {} (action={}, score={:.2})",
                suggestion.label,
                suggestion.action,
                suggestion.score
            );
            let socket = exec_socket.clone();
            let action = suggestion.action.clone();
            let params = suggestion.action_params.clone();
            let label = suggestion.label.clone();
            std::thread::spawn(move || {
                let _ = crate::ipc::send_command(
                    &socket,
                    "ExecuteAction",
                    &serde_json::json!({
                        "action": action,
                        "params": params,
                    }),
                );
                let label_clone = label.clone();
                let _ = glib::idle_add_local(move || {
                    // Signal via event channel instead of direct bar access.
                    tracing::info!("Palette action executed: {label_clone}");
                    glib::ControlFlow::Break
                });
            });
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

        // Wire up input entry text changes → debounced async context commands.
        //
        // Architecture:
        // - 40ms debounce (instant feel, prevents per-keystroke IPC spam)
        // - IPC runs on background thread (zero GTK main thread blocking)
        // - Query generation counter prevents stale responses overwriting newer results
        // - Channel-based result delivery (avoids Send/Sync issues with GTK widgets)
        // - Timing instrumentation in verbose mode
        let cp_socket = socket_path.to_string();
        let cp_palette = bar.command_palette.clone();
        let debounce_id: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let query_generation: std::rc::Rc<std::cell::Cell<u64>> =
            std::rc::Rc::new(std::cell::Cell::new(0));
        let (palette_tx, palette_rx) = mpsc::channel::<(
            u64,
            Result<Vec<crate::command_palette::CommandSuggestion>, String>,
        )>();

        // Poll palette results channel on GTK main loop.
        let poll_palette = cp_palette.clone();
        let poll_gen = query_generation.clone();
        glib::idle_add_local(move || {
            while let Ok((qgen, result)) = palette_rx.try_recv() {
                // Stale response check.
                if poll_gen.get() != qgen {
                    continue;
                }
                match result {
                    Ok(suggestions) => {
                        poll_palette.update_suggestions(suggestions);
                    }
                    Err(e) => {
                        tracing::warn!("Context commands fetch failed: {e}");
                        poll_palette.dismiss();
                    }
                }
                crate::timing::mark_render_end();
            }
            glib::ControlFlow::Continue
        });

        bar.input_entry.connect_changed(move |entry| {
            let text = entry.text().to_string();
            let socket = cp_socket.clone();
            let debounce_id = debounce_id.clone();
            let query_generation = query_generation.clone();
            let tx = palette_tx.clone();

            crate::timing::start_query(&text);

            // Cancel previous debounce timer.
            if let Some(id) = debounce_id.borrow_mut().take() {
                id.remove();
            }

            // Bump generation counter for this query.
            let this_generation = query_generation.get() + 1;
            query_generation.set(this_generation);

            // Debounce: 40ms after last keystroke before fetching.
            let new_id =
                glib::timeout_add_local_once(std::time::Duration::from_millis(40), move || {
                    let trimmed = text.trim();
                    if trimmed.len() < 2 {
                        return;
                    }

                    crate::timing::mark_debounce_end();

                    // Spawn background thread for IPC — zero GTK main thread blocking.
                    // Only Send-safe data moves into the thread.
                    let socket_bg = socket.clone();
                    let query_bg = trimmed.to_string();
                    let qgen_bg = this_generation;
                    let tx_bg = tx.clone();

                    crate::timing::mark_ipc_start();

                    std::thread::spawn(move || {
                        let result = crate::ipc::get_context_commands(&socket_bg, &query_bg, 6);

                        crate::timing::mark_ipc_end();

                        // Send result back to main thread via channel.
                        let _ = tx_bg.send((qgen_bg, result));
                    });
                });
            *debounce_id.borrow_mut() = Some(new_id);
        });

        // ── Wire welcome overlay chip buttons directly ────────
        // Each chip click: dismiss overlay, fill input + set thinking
        // on GTK main thread (click handler runs on main thread),
        // then send IPC calls in a bg thread with Send-safe data only.
        let wo_socket = socket_path.to_string();
        let wo_input = bar.input_entry.clone();
        let wo_bar = bar.clone();
        for (i, btn) in bar.welcome_overlay.chip_buttons.iter().enumerate() {
            let cmd = bar.welcome_overlay.chip_commands[i].clone();
            let s = wo_socket.clone();
            let input = wo_input.clone();
            let enabar = wo_bar.clone();
            btn.connect_clicked(move |_| {
                // Dismiss overlay (on GTK main thread).
                enabar.dismiss_welcome();
                // Fill input + show thinking state (GTK main thread).
                input.set_text(&cmd);
                enabar.set_state(BarState::Thinking);
                // IPC calls in bg thread — only Send-safe Strings.
                let sock = s.clone();
                let cmd_str = cmd.clone();
                std::thread::spawn(move || {
                    let _ = crate::ipc::send_command(
                        &sock,
                        "CompleteOnboarding",
                        &serde_json::json!({}),
                    );
                    let _ = crate::ipc::send_command(
                        &sock,
                        "ExecuteAction",
                        &serde_json::json!({"action": cmd_str, "params": {}}),
                    );
                });
            });
        }

        // Wire up mic button
        let b = bar.clone();
        bar.mic_button.connect_clicked(move |_| {
            tracing::info!("Mic button clicked");
            b.show_status("Listening...", 2);
        });

        // Wire restoration callbacks.
        let restore_bar = bar.clone();
        *bar.restoration.on_restore.lock().unwrap() = Some(Box::new(move |_snapshot_id| {
            restore_bar.is_restoring.store(true, Ordering::Relaxed);
            restore_bar.restoration.on_restore_started();
            tracing::info!("Restore triggered, waiting for orchestration plan");
        }));

        let dismiss_bar = bar.clone();
        *bar.restoration.on_dismiss.lock().unwrap() = Some(Box::new(move || {
            // Nothing extra needed, suggestion is hidden.
            tracing::info!("Restoration suggestion dismissed");
            let _ = &dismiss_bar;
        }));

        // Wire ambient suggestion callbacks.
        let amb_socket = socket_path.to_string();
        *bar.ambient.on_dismiss.lock().unwrap() = Some(Box::new(move |suggestion_id| {
            let path = amb_socket.clone();
            std::thread::spawn(move || {
                let _ = crate::ipc::send_command(
                    &path,
                    "DismissSuggestion",
                    &serde_json::json!({
                        "suggestion_id": suggestion_id,
                        "permanent": false,
                    }),
                );
            });
        }));

        let _act_socket = socket_path.to_string();
        *bar.ambient.on_act.lock().unwrap() =
            Some(Box::new(move |_suggestion_id, _action_type| {
                tracing::info!("Ambient action: {_action_type} on {_suggestion_id}");
                // Future: trigger action execution via IPC.
                // Dismiss is handled in the widget immediately.
            }));

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
                // Check for recent snapshots to suggest restoration.
                self.restoration.check_for_snapshots();
                // Check first-run status (async IPC to enad) — only once per session.
                if !self.first_run_checked.swap(true, Ordering::Relaxed) {
                    self.check_first_run();
                }
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
                        if let Some(hostname) = payload.get("hostname").and_then(|v| v.as_str()) {
                            self.status_label.set_label(&format!("enad: {hostname}"));
                        }
                    }
                    _ => {}
                }

                // Handle action lifecycle events.
                self.handle_action_event(&kind, &payload);

                // Handle orchestration plan/node events.
                self.handle_orchestration_event(&kind, &payload);

                // Handle ambient suggestion events.
                self.handle_ambient_event(&kind, &payload);
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

    /// Handle orchestration plan and node lifecycle events.
    fn handle_orchestration_event(&self, kind: &str, payload: &Value) {
        if kind != "System" {
            return;
        }

        let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "OrchestrationPlanEvent" => {
                if let Some((plan_id, status, message)) = parse_plan_event(payload) {
                    tracing::info!("Plan {plan_id}: {status} — {message}");

                    // Update plan-level status in the timeline.
                    match status.as_str() {
                        "PendingApproval" => {
                            // Build an initial display with just the plan info.
                            let display = OrchestrationDisplay {
                                plan_id: Some(plan_id.clone()),
                                plan_title: message
                                    .trim_start_matches("Plan requires approval")
                                    .trim()
                                    .to_string(),
                                status: "PendingApproval".to_string(),
                                message: "Requires approval".to_string(),
                                nodes: Vec::new(),
                            };
                            self.timeline.set_orchestration(display);
                            self.update_status_dot(0.85, 0.7, 0.2);
                        }
                        "Approved" => {
                            self.timeline.set_status("Approved", "Plan approved");
                            self.update_status_dot(0.2, 0.8, 0.3);
                        }
                        "Running" => {
                            self.timeline.set_status("Running", &message);
                            self.update_status_dot(0.85, 0.7, 0.2);
                        }
                        "Completed" => {
                            self.timeline.set_status("Completed", &message);
                            self.update_status_dot(0.2, 0.8, 0.3);
                            if self.is_restoring.load(Ordering::Relaxed) {
                                self.is_restoring.store(false, Ordering::Relaxed);
                                self.restoration.on_restore_completed();
                                // Auto-dismiss after 4 seconds.
                                let rest = self.restoration.clone();
                                glib::timeout_add_seconds_local(4, move || {
                                    rest.dismiss();
                                    glib::ControlFlow::Break
                                });
                            }
                        }
                        "Failed" => {
                            self.timeline.set_status("Failed", &message);
                            self.update_status_dot(0.8, 0.3, 0.3);
                            if self.is_restoring.load(Ordering::Relaxed) {
                                self.is_restoring.store(false, Ordering::Relaxed);
                                self.restoration.on_restore_failed();
                            }
                        }
                        "Cancelled" => {
                            self.timeline.set_status("Cancelled", &message);
                            self.update_status_dot(0.5, 0.5, 0.5);
                            if self.is_restoring.load(Ordering::Relaxed) {
                                self.is_restoring.store(false, Ordering::Relaxed);
                                self.restoration.dismiss();
                            }
                        }
                        "RollingBack" => {
                            self.timeline.set_status("RollingBack", &message);
                            self.update_status_dot(0.85, 0.6, 0.2);
                        }
                        "RolledBack" => {
                            self.timeline.set_status("RolledBack", &message);
                            self.update_status_dot(0.5, 0.5, 0.5);
                            if self.is_restoring.load(Ordering::Relaxed) {
                                self.is_restoring.store(false, Ordering::Relaxed);
                                self.restoration.dismiss();
                            }
                        }
                        _ => {}
                    }
                }
            }
            "OrchestrationNodeEvent" => {
                if let Some((_plan_id, node_id, status, error)) = parse_node_event(payload) {
                    tracing::info!("Node {node_id}: {status}");
                    self.timeline.update_node(&node_id, &status, "", error);
                }
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

    /// Handle ambient suggestion events from enad.
    fn handle_ambient_event(&self, kind: &str, payload: &Value) {
        if kind != "System" {
            return;
        }
        if let Some(suggestion) = parse_suggestion_event(payload) {
            tracing::info!(
                "Ambient suggestion: {} (priority={:.2})",
                suggestion.title,
                suggestion.priority
            );
            self.ambient.show(suggestion);
        }
    }

    /// Poll the restoration widget's IPC response channel.
    /// Also polls ambient widget auto-dismiss.
    /// Must be called from the GTK main thread (idle handler).
    pub fn poll_restoration(&self) {
        self.restoration.poll();
        self.ambient.poll_auto_dismiss();
    }

    /// Check first-run status from enad and show welcome overlay if needed.
    ///
    /// Uses a channel to pass the IPC result back to the GTK main thread,
    /// avoiding any Send/Sync issues with GTK widget types.
    /// Retries up to 30 times (3 seconds total) to handle startup delays.
    pub fn check_first_run(&self) {
        let socket = self.socket_path.clone();
        let wo = self.welcome_overlay.clone();

        // Channel: thread-safe bool transfer.
        let (tx, rx) = std::sync::mpsc::channel();

        // IPC in background thread — only Send-safe Strings.
        std::thread::spawn(move || {
            let result = crate::ipc::send_unit_command(&socket, "GetFirstRunStatus");
            let should_show = match result {
                Ok(response) => {
                    // Response envelope: {"id": "...", "kind": {"type": "Response", "body": {"Data": {"payload": {...}}}}}
                    let is_first = response
                        .pointer("/kind/body/Data/payload/is_first_launch")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let onboarding_done = response
                        .pointer("/kind/body/Data/payload/onboarding_completed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    is_first && !onboarding_done
                }
                Err(e) => {
                    tracing::info!("First-run check deferred (enad not ready): {e}");
                    false
                }
            };
            let _ = tx.send(should_show);
        });

        // Poll result on GTK main thread with retries.
        // Checks every 100ms, up to 30 times (3s window).
        // Unix socket IPC is typically <5ms, so this is generous.
        use std::cell::Cell;
        let retries = Cell::new(0);
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            retries.set(retries.get() + 1);
            if retries.get() > 30 {
                tracing::info!("First-run check exhausted (3s timeout)");
                return glib::ControlFlow::Break;
            }
            match rx.try_recv() {
                Ok(true) => {
                    wo.show();
                    glib::ControlFlow::Break
                }
                Ok(false) => {
                    tracing::info!("Not first launch — onboarding skipped");
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Not ready yet, retry.
                    glib::ControlFlow::Continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Sender dropped (IPC thread panicked or closed).
                    glib::ControlFlow::Break
                }
            }
        });
    }

    /// Access the welcome overlay for external dismissal.
    pub fn dismiss_welcome(&self) {
        self.welcome_overlay.dismiss();
    }

    /// Check if welcome overlay is showing.
    pub fn is_welcome_showing(&self) -> bool {
        self.welcome_overlay.is_showing()
    }

    /// Handle keyboard events for the command palette.
    /// Returns true if the event was consumed by the palette.
    pub fn handle_palette_key(&self, keyval: gdk::Key) -> bool {
        self.command_palette.handle_key(keyval)
    }
}
