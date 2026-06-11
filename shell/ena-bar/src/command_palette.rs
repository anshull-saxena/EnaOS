/// Command Palette — keyboard-first contextual command dropdown.
///
/// Design principles:
/// - Sparse, high-confidence suggestions (empty is correct)
/// - Keyboard-first navigation (↑↓ Enter Escape)
/// - Minimal visual density (max 6 suggestions)
/// - Stable suggestions (no flickering on identical results)
/// - Sub-10ms latency feel (debounced IPC, cached results)
use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use serde::{Deserialize, Serialize};

/// A single command suggestion from the context engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSuggestion {
    pub id: String,
    pub label: String,
    pub subtitle: String,
    pub icon: String,
    pub action: String,
    pub action_params: serde_json::Value,
    pub score: f64,
    pub source: String,
}

/// Internal state for a single suggestion row.
struct SuggestionRow {
    container: gtk4::Box,
    icon_label: gtk4::Label,
    main_label: gtk4::Label,
    subtitle_label: gtk4::Label,
}

/// The CommandPalette widget.
pub struct CommandPalette {
    pub container: gtk4::Box,
    revealer: gtk4::Revealer,
    suggestions_box: gtk4::Box,
    preview_label: gtk4::Label,
    preview_revealer: gtk4::Revealer,
    rows: Rc<RefCell<Vec<SuggestionRow>>>,
    suggestions: Rc<RefCell<Vec<CommandSuggestion>>>,
    selected_index: Rc<RefCell<Option<usize>>>,
    on_select: Rc<RefCell<Option<Box<dyn Fn(&CommandSuggestion)>>>>,
}

impl CommandPalette {
    pub fn new() -> Self {
        let suggestions_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(2)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(6)
            .margin_end(6)
            .css_classes(["ena-palette-list"])
            .build();

        // Execution preview label — shows intent transparency before execution.
        let preview_label = gtk4::Label::builder()
            .label("")
            .xalign(0.0)
            .margin_top(2)
            .margin_bottom(4)
            .margin_start(10)
            .margin_end(10)
            .css_classes(["ena-palette-preview"])
            .build();

        let preview_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(120)
            .child(&preview_label)
            .build();

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(150)
            .child(&suggestions_box)
            .build();

        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .css_classes(["ena-palette-container"])
            .build();
        container.append(&revealer);
        container.append(&preview_revealer);

        Self {
            container,
            revealer,
            suggestions_box,
            preview_label,
            preview_revealer,
            rows: Rc::new(RefCell::new(Vec::new())),
            suggestions: Rc::new(RefCell::new(Vec::new())),
            selected_index: Rc::new(RefCell::new(None)),
            on_select: Rc::new(RefCell::new(None)),
        }
    }

    /// Set callback fired when a suggestion is selected (Enter key or click).
    pub fn set_on_select<F: Fn(&CommandSuggestion) + 'static>(&self, cb: F) {
        *self.on_select.borrow_mut() = Some(Box::new(cb));
    }

    /// Update the palette with new suggestions.
    ///
    /// Stability strategy:
    /// - Only rebuilds if suggestion IDs differ from last render
    /// - Preserves selection position when current selection still exists
    /// - Resets to index 0 only when the top suggestion changes
    /// - No flickering on identical or near-identical results
    pub fn update_suggestions(&self, suggestions: Vec<CommandSuggestion>) {
        let suggestions: Vec<_> = suggestions.into_iter().take(6).collect();

        // Stability check — compare IDs to avoid flickering.
        let current_ids: Vec<String> = self
            .rows
            .borrow()
            .iter()
            .filter_map(|r| {
                r.container
                    .widget_name()
                    .strip_prefix("row-")
                    .map(String::from)
            })
            .collect();
        let new_ids: Vec<String> = suggestions.iter().map(|s| s.id.clone()).collect();

        if current_ids == new_ids && !new_ids.is_empty() {
            // Identical suggestions — no update needed.
            return;
        }

        // Check if this is a "stable subset" update:
        // new IDs are a prefix of current IDs (user typed more, narrowed results).
        // In this case, preserve selection if the selected item is still present.
        let is_stable_subset = !new_ids.is_empty()
            && !current_ids.is_empty()
            && new_ids.iter().all(|id| current_ids.contains(id));

        // Clear existing rows.
        self.clear_rows();

        if suggestions.is_empty() {
            self.revealer.set_reveal_child(false);
            *self.selected_index.borrow_mut() = None;
            return;
        }

        // Store suggestions for callback access.
        *self.suggestions.borrow_mut() = suggestions.clone();

        // Build new rows.
        let mut rows = Vec::with_capacity(suggestions.len());
        for (idx, suggestion) in suggestions.iter().enumerate() {
            let row = self.build_row(suggestion, idx);
            self.suggestions_box.append(&row.container);
            rows.push(row);
        }

        // Selection stability: preserve position if possible.
        let prev_selected = if is_stable_subset {
            // Try to keep the previously selected item.
            let prev_idx = *self.selected_index.borrow();
            prev_idx.filter(|i| *i < rows.len())
        } else {
            // New set of suggestions — select first item.
            Some(0)
        };

        *self.selected_index.borrow_mut() = prev_selected;
        if let Some(idx) = prev_selected
            && let Some(row) = rows.get(idx)
        {
            row.container.add_css_class("ena-palette-row-selected");
        }

        // Show execution preview for initial selection.
        self.update_preview(prev_selected);

        self.revealer.set_reveal_child(true);
    }

    /// Handle keyboard navigation. Returns true if event was consumed.
    ///
    /// Keyboard ergonomics:
    /// - ↑↓ : Navigate suggestions
    /// - Enter : Execute selected suggestion + dismiss
    /// - Tab : Accept first suggestion + dismiss
    /// - Escape : Dismiss palette
    pub fn handle_key(&self, keyval: gdk::Key) -> bool {
        if !self.revealer.reveals_child() {
            return false;
        }

        let rows = self.rows.borrow();
        if rows.is_empty() {
            // Palette visible but no suggestions — only handle Escape.
            return matches!(keyval, gdk::Key::Escape);
        }

        let mut selected = self.selected_index.borrow_mut();

        match keyval {
            gdk::Key::Down => {
                let next = selected.map_or(0, |i| (i + 1).min(rows.len() - 1));
                self.set_selected(&rows, *selected, Some(next));
                *selected = Some(next);
                true
            }
            gdk::Key::Up => {
                let prev = selected.map_or(0, |i| i.saturating_sub(1));
                self.set_selected(&rows, *selected, Some(prev));
                *selected = Some(prev);
                true
            }
            gdk::Key::Tab => {
                // Tab accepts the first suggestion.
                if let Some(suggestion) = self.suggestions.borrow().first()
                    && let Some(ref cb) = *self.on_select.borrow()
                {
                    cb(suggestion);
                }
                self.dismiss();
                true
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if let Some(idx) = *selected {
                    let suggestions = self.suggestions.borrow();
                    if let Some(suggestion) = suggestions.get(idx)
                        && let Some(ref cb) = *self.on_select.borrow()
                    {
                        cb(suggestion);
                    }
                }
                self.dismiss();
                true
            }
            gdk::Key::Escape => {
                self.dismiss();
                true
            }
            _ => false,
        }
    }

    /// Dismiss the palette.
    pub fn dismiss(&self) {
        self.revealer.set_reveal_child(false);
        self.preview_revealer.set_reveal_child(false);
        self.clear_rows();
        *self.selected_index.borrow_mut() = None;
    }

    /// Update the execution preview label.
    fn update_preview(&self, idx: Option<usize>) {
        if let Some(idx) = idx {
            let suggestions = self.suggestions.borrow();
            if let Some(suggestion) = suggestions.get(idx) {
                // Show action preview: "↳ {action} — {source}"
                let preview = format!(
                    "\u{21B3} {} \u{2014} {}",
                    suggestion.action, suggestion.source
                );
                self.preview_label.set_label(&preview);
                self.preview_revealer.set_reveal_child(true);
                return;
            }
        }
        self.preview_revealer.set_reveal_child(false);
    }

    /// Check if palette is currently visible.
    pub fn is_visible(&self) -> bool {
        self.revealer.reveals_child()
    }

    /// Get the currently selected suggestion index.
    pub fn selected_index(&self) -> Option<usize> {
        *self.selected_index.borrow()
    }

    /// Clear all suggestion rows from the UI.
    fn clear_rows(&self) {
        while let Some(child) = self.suggestions_box.first_child() {
            self.suggestions_box.remove(&child);
        }
        self.rows.borrow_mut().clear();
        *self.suggestions.borrow_mut() = Vec::new();
    }

    /// Build a single suggestion row.
    fn build_row(&self, suggestion: &CommandSuggestion, _index: usize) -> SuggestionRow {
        let icon_label = gtk4::Label::builder()
            .label(self.icon_for(&suggestion.icon))
            .css_classes(["ena-palette-icon"])
            .build();

        let main_label = gtk4::Label::builder()
            .label(&suggestion.label)
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["ena-palette-label"])
            .build();

        let subtitle_label = gtk4::Label::builder()
            .label(&suggestion.subtitle)
            .xalign(1.0)
            .css_classes(["ena-palette-subtitle"])
            .build();

        let content_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .hexpand(true)
            .build();
        content_box.append(&icon_label);
        content_box.append(&main_label);
        content_box.append(&subtitle_label);

        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(0)
            .css_classes(["ena-palette-row"])
            .build();
        container.set_widget_name(&format!("row-{}", suggestion.id));
        container.append(&content_box);

        // Click handler via GestureClick.
        let suggestions = self.suggestions.clone();
        let on_select = self.on_select.clone();
        let rows = self.rows.clone();
        let selected_index = self.selected_index.clone();
        let revealer = self.revealer.clone();
        let click_idx = _index;
        let gesture = gtk4::GestureClick::new();
        gesture.connect_pressed(move |_gesture, _n_press, _x, _y| {
            let suggestions_ref = suggestions.borrow();
            if let Some(suggestion) = suggestions_ref.get(click_idx)
                && let Some(ref cb) = *on_select.borrow()
            {
                cb(suggestion);
            }
            // Clear selection state.
            {
                let rows_ref = rows.borrow();
                if let Some(idx) = *selected_index.borrow()
                    && let Some(row) = rows_ref.get(idx)
                {
                    row.container.remove_css_class("ena-palette-row-selected");
                }
            }
            *selected_index.borrow_mut() = None;
            revealer.set_reveal_child(false);
        });
        container.add_controller(gesture);

        SuggestionRow {
            container,
            icon_label,
            main_label,
            subtitle_label,
        }
    }

    /// Update visual selection state.
    fn set_selected(&self, rows: &[SuggestionRow], old: Option<usize>, new: Option<usize>) {
        if let Some(idx) = old
            && let Some(row) = rows.get(idx)
        {
            row.container.remove_css_class("ena-palette-row-selected");
        }
        if let Some(idx) = new
            && let Some(row) = rows.get(idx)
        {
            row.container.add_css_class("ena-palette-row-selected");
        }
        // Update execution preview.
        self.update_preview(new);
    }

    /// Map icon identifiers to Unicode symbols.
    fn icon_for(&self, icon: &str) -> String {
        match icon {
            "workflow" => "\u{2699}",
            "snapshot" => "\u{1F4F7}",
            "restore" => "\u{21A9}",
            "app" => "\u{25A0}",
            "search" => "\u{1F50D}",
            "action" => "\u{26A1}",
            "context" => "\u{1F310}",
            _ => "\u{25B6}",
        }
        .to_string()
    }
}
