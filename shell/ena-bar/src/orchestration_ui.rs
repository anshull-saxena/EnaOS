use std::sync::Arc;
use std::sync::Mutex;

use gtk4::prelude::*;
use gtk4::glib;
use serde_json::Value;

/// Display state for a single orchestration node.
#[derive(Debug, Clone)]
pub(crate) struct NodeDisplay {
    pub id: String,
    pub label: String,
    pub status: String,   // Pending, Running, Completed, Failed, Skipped, Cancelled
    pub error: Option<String>,
}

/// Overall orchestration display state.
#[derive(Debug, Clone, Default)]
pub(crate) struct OrchestrationDisplay {
    pub plan_id: Option<String>,
    pub plan_title: String,
    pub status: String,   // Hidden, PendingApproval, Running, Completed, Failed, etc.
    pub message: String,
    pub nodes: Vec<NodeDisplay>,
}

/// Timeline widget — renders orchestration execution as a compact vertical list.
///
/// Three display modes:
/// - Approval: shows plan title + approve/reject buttons
/// - Active: shows live timeline of nodes with status icons
/// - Summary: compact "3/5 steps completed" after completion
pub struct TimelineWidget {
    pub container: gtk4::Box,

    // Approval bar.
    approval_revealer: gtk4::Revealer,
    approval_label: gtk4::Label,
    approve_button: gtk4::Button,
    reject_button: gtk4::Button,

    // Timeline node list.
    list_revealer: gtk4::Revealer,
    node_list: gtk4::ListBox,

    // Summary.
    summary_revealer: gtk4::Revealer,
    summary_label: gtk4::Label,

    // Internal state.
    state: Mutex<OrchestrationDisplay>,

    // Approval callback (set by bar.rs).
    pub on_approve: Mutex<Option<Box<dyn Fn(String) + Send>>>,
    pub on_reject: Mutex<Option<Box<dyn Fn(String) + Send>>>,
    pub on_cancel: Mutex<Option<Box<dyn Fn(String) + Send>>>,
}

impl TimelineWidget {
    pub fn new() -> Arc<Self> {
        // ── Approval bar ─────────────────────────────────────────
        let approval_label = gtk4::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .build();
        approval_label.add_css_class("ena-orch-approval-label");

        let approve_button = gtk4::Button::with_label("Approve");
        approve_button.add_css_class("ena-orch-approve-btn");
        let approve_button_clone = approve_button.clone();

        let reject_button = gtk4::Button::with_label("Reject");
        reject_button.add_css_class("ena-orch-reject-btn");
        let reject_button_clone = reject_button.clone();

        let approval_button_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::End)
            .build();
        approval_button_box.append(&approve_button);
        approval_button_box.append(&reject_button);

        let approval_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        approval_box.append(&approval_label);
        approval_box.append(&approval_button_box);

        let approval_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .child(&approval_box)
            .build();

        // ── Node timeline list ──────────────────────────────────
        let node_list = gtk4::ListBox::builder()
            .activate_on_single_click(false)
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        node_list.add_css_class("ena-orch-list");

        let list_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(250)
            .child(&node_list)
            .build();

        // ── Summary ──────────────────────────────────────────────
        let summary_label = gtk4::Label::builder()
            .xalign(0.0)
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        summary_label.add_css_class("ena-orch-summary");

        let summary_revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .child(&summary_label)
            .build();

        // ── Root container ──────────────────────────────────────
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .css_classes(["ena-orch-container"])
            .build();
        container.append(&approval_revealer);
        container.append(&list_revealer);
        container.append(&summary_revealer);

        let widget = Arc::new(TimelineWidget {
            container,
            approval_revealer,
            approval_label,
            approve_button,
            reject_button,
            list_revealer,
            node_list,
            summary_revealer,
            summary_label,
            state: Mutex::new(OrchestrationDisplay::default()),
            on_approve: Mutex::new(None),
            on_reject: Mutex::new(None),
            on_cancel: Mutex::new(None),
        });

        // Wire approval buttons.
        let w = widget.clone();
        approve_button_clone.connect_clicked(move |_| {
            let state = w.state.lock().unwrap();
            if let Some(ref plan_id) = state.plan_id {
                if let Some(ref cb) = *w.on_approve.lock().unwrap() {
                    cb(plan_id.clone());
                }
            }
        });

        let w = widget.clone();
        reject_button_clone.connect_clicked(move |_| {
            let state = w.state.lock().unwrap();
            if let Some(ref plan_id) = state.plan_id {
                if let Some(ref cb) = *w.on_reject.lock().unwrap() {
                    cb(plan_id.clone());
                }
            }
        });

        widget
    }

    /// Set the full orchestration display state.
    pub fn set_orchestration(&self, display: OrchestrationDisplay) {
        *self.state.lock().unwrap() = display.clone();
        self.render(&display);
    }

    /// Update the plan-level status (Running, Completed, Failed, etc.).
    pub fn set_status(&self, status: &str, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.status = status.to_string();
        state.message = message.to_string();
        let display = state.clone();
        drop(state);
        self.render(&display);
    }

    /// Update a single node's status.
    pub fn update_node(&self, node_id: &str, status: &str, label: &str, error: Option<String>) {
        let mut state = self.state.lock().unwrap();
        if let Some(node) = state.nodes.iter_mut().find(|n| n.id == node_id) {
            node.status = status.to_string();
            node.label = label.to_string();
            node.error = error;
        }
        let display = state.clone();
        drop(state);
        self.render(&display);
    }

    /// Render the current state into widgets.
    fn render(&self, display: &OrchestrationDisplay) {
        match display.status.as_str() {
            "PendingApproval" => {
                self.show_approval(display);
                self.list_revealer.set_reveal_child(false);
                self.summary_revealer.set_reveal_child(false);
            }
            "Running" | "Approved" | "RollingBack" => {
                self.approval_revealer.set_reveal_child(false);
                self.render_node_list(display);
                self.list_revealer.set_reveal_child(true);
                self.summary_revealer.set_reveal_child(false);
            }
            "Completed" | "Failed" | "Cancelled" | "RolledBack" => {
                self.approval_revealer.set_reveal_child(false);
                self.list_revealer.set_reveal_child(true);
                self.render_summary(display);
                self.summary_revealer.set_reveal_child(true);

                // Auto-hide after 6 seconds for transient visibility.
                let list_rev = self.list_revealer.clone();
                let summary_rev = self.summary_revealer.clone();
                glib::timeout_add_seconds_local(6, move || {
                    list_rev.set_reveal_child(false);
                    summary_rev.set_reveal_child(false);
                    glib::ControlFlow::Break
                });
            }
            _ => {
                self.approval_revealer.set_reveal_child(false);
                self.list_revealer.set_reveal_child(false);
                self.summary_revealer.set_reveal_child(false);
            }
        }
    }

    /// Show approval prompt with plan title + buttons.
    fn show_approval(&self, display: &OrchestrationDisplay) {
        let text = format!("Plan: {} — requires approval", display.plan_title);
        self.approval_label.set_label(&text);
        self.approval_revealer.set_reveal_child(true);

        // Detect risky nodes and add them to the approval text.
        let risky: Vec<&str> = display
            .nodes
            .iter()
            .filter(|n| n.status == "Pending")
            .map(|n| n.label.as_str())
            .collect();
        if !risky.is_empty() {
            let detail = format!("\nActions: {}", risky.join(", "));
            self.approval_label.set_label(&format!("{}{}", text, detail));
        }
    }

    /// Build and show the timeline node list.
    fn render_node_list(&self, display: &OrchestrationDisplay) {
        // Clear existing rows.
        while let Some(child) = self.node_list.first_child() {
            self.node_list.remove(&child);
        }

        let completed = display.nodes.iter().filter(|n| n.status == "Completed").count();
        let total = display.nodes.len();

        // Header: plan title + progress.
        if !display.plan_title.is_empty() {
            let header = gtk4::Label::builder()
                .label(&format!(
                    "{}  —  {}/{} steps",
                    display.plan_title, completed, total
                ))
                .xalign(0.0)
                .margin_start(12)
                .margin_end(12)
                .margin_top(6)
                .margin_bottom(2)
                .build();
            header.add_css_class("ena-orch-header");
            self.node_list.append(&header);
        }

        for node in &display.nodes {
            let row = self.build_node_row(node);
            self.node_list.append(&row);
        }

        // Status message at bottom.
        if !display.message.is_empty() {
            let msg = gtk4::Label::builder()
                .label(&display.message)
                .xalign(0.0)
                .margin_start(12)
                .margin_end(12)
                .margin_top(2)
                .margin_bottom(6)
                .build();
            msg.add_css_class("ena-orch-msg");
            self.node_list.append(&msg);
        }

        self.node_list.set_visible(true);
    }

    /// Build a single node row widget.
    fn build_node_row(&self, node: &NodeDisplay) -> gtk4::Box {
        let (icon, icon_class) = match node.status.as_str() {
            "Pending" => ("○", "ena-orch-pending"),
            "Running" => ("●", "ena-orch-running"),
            "Completed" => ("✓", "ena-orch-done"),
            "Failed" => ("✗", "ena-orch-failed"),
            "Skipped" => ("—", "ena-orch-skipped"),
            "Cancelled" => ("⊘", "ena-orch-cancelled"),
            _ => ("○", "ena-orch-pending"),
        };

        let icon_label = gtk4::Label::builder()
            .label(icon)
            .width_request(20)
            .xalign(0.5)
            .build();
        icon_label.add_css_class(icon_class);
        icon_label.add_css_class("ena-orch-icon");

        let text = match node.status.as_str() {
            "Failed" => {
                let err = node.error.as_deref().unwrap_or("error");
                format!("{}  —  {}", node.label, err)
            }
            "Running" => format!("{}…", node.label),
            _ => node.label.clone(),
        };

        let text_label = gtk4::Label::builder()
            .label(&text)
            .xalign(0.0)
            .hexpand(true)
            .margin_start(4)
            .build();
        text_label.add_css_class("ena-orch-node-label");

        // Retry indicator.
        let row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .margin_start(12)
            .margin_end(12)
            .margin_top(2)
            .margin_bottom(2)
            .build();
        row.append(&icon_label);
        row.append(&text_label);

        row
    }

    /// Show compact completion summary.
    fn render_summary(&self, display: &OrchestrationDisplay) {
        let total = display.nodes.len();
        let completed = display.nodes.iter().filter(|n| n.status == "Completed").count();
        let failed = display.nodes.iter().filter(|n| n.status == "Failed").count();
        let skipped = display.nodes.iter().filter(|n| n.status == "Skipped").count();

        let mut text = match display.status.as_str() {
            "Completed" => format!("✓ Plan completed — {completed}/{total} steps"),
            "Failed" => format!("✗ Plan failed — {completed}/{total} steps, {failed} failed"),
            "Cancelled" => format!("⊘ Plan cancelled — {completed}/{total} completed"),
            "RolledBack" => format!("⟳ Rolled back — {skipped} skipped"),
            _ => format!("{}/{} steps", completed, total),
        };

        if !display.message.is_empty() {
            text = format!("{}  ({})", text, display.message);
        }

        self.summary_label.set_label(&text);
    }

    /// Hide all orchestration widgets.
    pub fn hide_all(&self) {
        self.approval_revealer.set_reveal_child(false);
        self.list_revealer.set_reveal_child(false);
        self.summary_revealer.set_reveal_child(false);
        *self.state.lock().unwrap() = OrchestrationDisplay::default();
    }

    /// Check if any orchestration content is visible.
    pub fn is_visible(&self) -> bool {
        self.approval_revealer.reveals_child()
            || self.list_revealer.reveals_child()
            || self.summary_revealer.reveals_child()
    }
}

// ── Event parsing helpers ──────────────────────────────────────

/// Parse an OrchestrationPlanEvent payload from enad.
pub(crate) fn parse_plan_event(payload: &Value) -> Option<(String, String, String)> {
    let data = payload.get("data")?;
    let plan_id = data.get("plan_id").and_then(|v| v.as_str())?;
    let status = data.get("status").and_then(|v| v.as_str())?;
    let message = data.get("message").and_then(|v| v.as_str()).unwrap_or("");
    Some((plan_id.to_string(), status.to_string(), message.to_string()))
}

/// Parse an OrchestrationNodeEvent payload from enad.
pub(crate) fn parse_node_event(payload: &Value) -> Option<(String, String, String, Option<String>)> {
    let data = payload.get("data")?;
    let plan_id = data.get("plan_id").and_then(|v| v.as_str())?;
    let node_id = data.get("node_id").and_then(|v| v.as_str())?;
    let node_status = data.get("status").and_then(|v| v.as_str())?;
    let _label = data.get("label").and_then(|v| v.as_str()).unwrap_or("");
    let error = data.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());
    Some((
        plan_id.to_string(),
        node_id.to_string(),
        node_status.to_string(),
        error,
    ))
}
