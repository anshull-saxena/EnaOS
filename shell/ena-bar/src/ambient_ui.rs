use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use gtk4::prelude::*;
use serde_json::Value;

/// A single ambient suggestion from the daemon.
#[derive(Debug, Clone, Default)]
pub(crate) struct AmbientSuggestion {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub priority: f64,
    pub action_label: Option<String>,
    pub action_type: Option<String>,
}

/// Widget state for what's currently displayed.
enum DisplayState {
    Hidden,
    Showing {
        suggestion: AmbientSuggestion,
        expiry_ms: u64,
        started_at: Instant,
    },
}

/// Ambient suggestion widget.
///
/// Displays one non-intrusive suggestion at a time with fade-in/out.
/// Auto-dismisses after a configurable duration based on priority.
pub struct AmbientSuggestionWidget {
    pub container: gtk4::Box,
    revealer: gtk4::Revealer,
    icon_label: gtk4::Label,
    text_label: gtk4::Label,
    action_button: gtk4::Button,
    dismiss_button: gtk4::Button,
    state: Mutex<DisplayState>,

    /// Callbacks set by bar.rs. Called on GTK main thread.
    pub on_dismiss: Mutex<Option<Box<dyn Fn(String)>>>,
    pub on_act: Mutex<Option<Box<dyn Fn(String, String)>>>,
}

impl AmbientSuggestionWidget {
    pub fn new() -> Arc<Self> {
        let icon_label = gtk4::Label::builder()
            .label("\u{1F4A1}")
            .width_request(20)
            .xalign(0.5)
            .build();
        icon_label.add_css_class("ena-ambient-icon");

        let text_label = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        text_label.add_css_class("ena-ambient-text");

        let action_button = gtk4::Button::new();
        action_button.add_css_class("ena-ambient-action-btn");
        action_button.set_visible(false);

        let dismiss_button = gtk4::Button::builder()
            .label("\u{00D7}")
            .css_classes(["ena-ambient-dismiss-btn"])
            .build();

        let row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        row.append(&icon_label);
        row.append(&text_label);
        row.append(&action_button);
        row.append(&dismiss_button);

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::Crossfade)
            .transition_duration(300)
            .child(&row)
            .build();

        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .css_classes(["ena-ambient-container"])
            .build();
        container.append(&revealer);

        let widget = Arc::new(AmbientSuggestionWidget {
            container,
            revealer,
            icon_label,
            text_label,
            action_button,
            dismiss_button,
            state: Mutex::new(DisplayState::Hidden),
            on_dismiss: Mutex::new(None),
            on_act: Mutex::new(None),
        });

        // Wire dismiss button.
        let w = widget.clone();
        widget.dismiss_button.connect_clicked(move |_| {
            let sid = w.current_suggestion_id();
            if !sid.is_empty() {
                w.dismiss_current(&sid, false);
            }
        });

        // Wire action button.
        let w = widget.clone();
        widget.action_button.connect_clicked(move |_| {
            let (sid, at) = w.current_action();
            if !sid.is_empty() {
                if let Some(ref cb) = *w.on_act.lock().unwrap() {
                    cb(sid.clone(), at);
                }
                w.dismiss_current(&sid, true);
            }
        });

        widget
    }

    /// Get current suggestion ID.
    fn current_suggestion_id(&self) -> String {
        let state = self.state.lock().unwrap();
        match &*state {
            DisplayState::Showing { suggestion, .. } => suggestion.id.clone(),
            DisplayState::Hidden => String::new(),
        }
    }

    /// Get current suggestion action info.
    fn current_action(&self) -> (String, String) {
        let state = self.state.lock().unwrap();
        match &*state {
            DisplayState::Showing { suggestion, .. } => {
                (suggestion.id.clone(), suggestion.action_type.clone().unwrap_or_default())
            }
            DisplayState::Hidden => (String::new(), String::new()),
        }
    }

    /// Show a suggestion. Smoothly replaces any current one.
    pub fn show(&self, suggestion: AmbientSuggestion) {
        let expiry_ms = Self::duration_for(&suggestion);

        *self.state.lock().unwrap() = DisplayState::Showing {
            suggestion: suggestion.clone(),
            expiry_ms,
            started_at: Instant::now(),
        };

        self.text_label.set_label(&suggestion.title);

        if let Some(ref label) = suggestion.action_label {
            self.action_button.set_label(label);
            self.action_button.set_visible(true);
        } else {
            self.action_button.set_visible(false);
        }

        self.revealer.set_reveal_child(true);
    }

    /// Duration a suggestion should be visible, based on priority.
    fn duration_for(suggestion: &AmbientSuggestion) -> u64 {
        let base = 5000u64;
        let bonus = (suggestion.priority * 7000.0) as u64;
        (base + bonus).min(12000)
    }

    /// Poll auto-dismiss. Call from idle handler.
    pub fn poll_auto_dismiss(&self) {
        let mut state = self.state.lock().unwrap();
        if let DisplayState::Showing { started_at, expiry_ms, .. } = &*state {
            if started_at.elapsed().as_millis() as u64 >= *expiry_ms {
                let sid = match &*state {
                    DisplayState::Showing { suggestion, .. } => suggestion.id.clone(),
                    _ => return,
                };
                *state = DisplayState::Hidden;
                drop(state);
                self.revealer.set_reveal_child(false);
                if let Some(ref cb) = *self.on_dismiss.lock().unwrap() {
                    cb(sid);
                }
            }
        }
    }

    /// Dismiss the current suggestion.
    pub fn dismiss_current(&self, suggestion_id: &str, _acted: bool) {
        *self.state.lock().unwrap() = DisplayState::Hidden;
        self.revealer.set_reveal_child(false);
        if let Some(ref cb) = *self.on_dismiss.lock().unwrap() {
            cb(suggestion_id.to_string());
        }
    }

    /// Check if any suggestion is currently visible.
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.revealer.reveals_child()
    }
}

/// Parse a suggestion from an IPC system event payload.
pub(crate) fn parse_suggestion_event(payload: &Value) -> Option<AmbientSuggestion> {
    let data = payload.get("data")?;
    // EventPayload is adjacently tagged: {"type": "SuggestionGenerated", "data": {...}}
    // The type discriminator is at the payload level, not inside data.
    let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type != "SuggestionGenerated" {
        return None;
    }
    // The suggestion data is in the payload after the type discriminator.
    // Event payload format: {"type": "SuggestionGenerated", "data": {...}}
    Some(AmbientSuggestion {
        id: data.get("suggestion_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        kind: data.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        title: data.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        description: data.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        priority: data.get("priority").and_then(|v| v.as_f64()).unwrap_or(0.0),
        action_label: data.get("action_label").and_then(|v| v.as_str()).map(|s| s.to_string()),
        action_type: data.get("action_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}
