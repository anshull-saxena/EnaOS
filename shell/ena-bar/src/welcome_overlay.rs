/// Welcome Overlay — subtle first-run intro for EnaOS.
///
/// Design:
/// - Not a modal, not a tutorial, not a chatbot
/// - A subtle GTK4 overlay that crossfades in on first launch
/// - Shows EnaOS wordmark + three contextual suggestion chips
/// - Dismisses on chip click, Escape, or 12s auto-timeout
/// - Once dismissed, never shows again (permanent marker via enad)
///
/// The overlay lives above the bar row inside the container, revealed
/// with a smooth crossfade transition.
///
/// **Thread safety**: WelcomeOverlay itself is NOT Send/Sync because it
/// contains GTK widgets. All methods must be called from the GTK main thread.
/// Bar.rs wires chip click handlers directly using glib closures, avoiding
/// the Send bound issue by keeping all GTK access on the main thread.
use gtk4::glib;
use gtk4::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum OverlayState {
    Hidden,
    Showing,
    Dismissed,
}

/// The welcome overlay widget.
pub struct WelcomeOverlay {
    pub container: gtk4::Revealer,
    state: std::cell::Cell<OverlayState>,
    /// Chip buttons exposed to bar.rs for direct signal wiring.
    pub chip_buttons: Vec<gtk4::Button>,
    /// Commands corresponding to each chip button.
    pub chip_commands: Vec<String>,
}

impl WelcomeOverlay {
    pub fn new() -> Self {
        // ── Wordmark ─────────────────────────────────────────────
        let wordmark = gtk4::Label::builder()
            .label("EnaOS")
            .xalign(0.5)
            .yalign(0.0)
            .css_classes(["ena-welcome-wordmark"])
            .build();

        // ── Tagline ──────────────────────────────────────────────
        let tagline = gtk4::Label::builder()
            .label("Your environment knows what matters.")
            .xalign(0.5)
            .css_classes(["ena-welcome-tagline"])
            .build();

        // ── Suggestion chips ─────────────────────────────────────
        // Each chip corresponds to a real enad IPC command.
        // Label is user-facing, command is the IPC payload.
        let chip_specs: [(&str, &str); 3] = [
            ("open browser", "Open Browser"),
            ("check system status", "Check System"),
            ("create a snapshot", "Take Snapshot"),
        ];

        let chip_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk4::Align::Center)
            .margin_top(12)
            .build();

        let mut chip_buttons = Vec::new();
        let mut chip_commands = Vec::new();

        for (command, label) in &chip_specs {
            let btn = gtk4::Button::builder()
                .label(*label)
                .css_classes(["ena-welcome-chip"])
                .build();
            chip_buttons.push(btn.clone());
            chip_commands.push(command.to_string());
            chip_box.append(&btn);
        }

        // ── Dismiss hint ─────────────────────────────────────────
        let dismiss_hint = gtk4::Label::builder()
            .label("Press Escape or type to get started")
            .xalign(0.5)
            .margin_top(16)
            .css_classes(["ena-welcome-hint"])
            .build();

        // ── Layout ───────────────────────────────────────────────
        let layout = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .margin_top(20)
            .margin_bottom(12)
            .margin_start(24)
            .margin_end(24)
            .build();
        layout.append(&wordmark);
        layout.append(&tagline);
        layout.append(&chip_box);
        layout.append(&dismiss_hint);

        let overlay_frame = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .css_classes(["ena-welcome-overlay"])
            .build();
        overlay_frame.append(&layout);

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::Crossfade)
            .transition_duration(400)
            .child(&overlay_frame)
            .build();

        WelcomeOverlay {
            container: revealer,
            state: std::cell::Cell::new(OverlayState::Hidden),
            chip_buttons,
            chip_commands,
        }
    }

    /// Show the welcome overlay with crossfade animation.
    /// Auto-dismisses after 12 seconds.
    pub fn show(&self) {
        if self.state.get() == OverlayState::Dismissed {
            return;
        }
        self.state.set(OverlayState::Showing);
        self.container.set_reveal_child(true);
        self.container.set_visible(true);

        // Auto-dismiss after 12 seconds (timeout starts when shown).
        let weak = self.container.downgrade();
        glib::timeout_add_seconds_local(12, move || {
            if let Some(container) = weak.upgrade() {
                container.set_reveal_child(false);
            }
            glib::ControlFlow::Break
        });
    }

    /// Dismiss the overlay with fade animation.
    /// After animation completes, sets visible=false.
    pub fn dismiss(&self) {
        if self.state.get() == OverlayState::Dismissed {
            return;
        }
        self.state.set(OverlayState::Dismissed);
        self.container.set_reveal_child(false);

        // After animation completes, hide.
        let weak = self.container.downgrade();
        glib::timeout_add_seconds_local(1, move || {
            if let Some(container) = weak.upgrade() {
                container.set_visible(false);
            }
            glib::ControlFlow::Break
        });
    }

    /// Check if the overlay is currently showing.
    pub fn is_showing(&self) -> bool {
        self.state.get() == OverlayState::Showing
    }

    /// Check if the overlay has been dismissed this session.
    #[allow(dead_code)]
    pub fn is_dismissed(&self) -> bool {
        self.state.get() == OverlayState::Dismissed
    }
}
