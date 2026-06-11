use std::sync::Arc;

use chrono::{DateTime, Duration, Timelike, Utc};
use serde_json::Value;
use tracing::{info, warn};
use uuid::Uuid;

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};
use crate::types::ipc::Response;

use super::store::SuggestionStore;
use super::types::{ContextEvent, ContextWindow, Suggestion, SuggestionAction, SuggestionKind};

/// Minimum priority threshold for a suggestion to be surfaced.
const MIN_PRIORITY: f64 = 0.4;

/// Default suggestion expiry from creation.
const DEFAULT_TTL_MINUTES: i64 = 20;

/// The ambient suggestion engine.
///
/// Watches system events, builds a context window, and produces
/// lightweight suggestions using deterministic rules.
pub struct SuggestionEngine {
    store: Arc<SuggestionStore>,
    bus: Arc<EventBus>,
    /// Recent events for context window building.
    recent_events: std::sync::Mutex<Vec<ContextEvent>>,
    /// Last suggestion time per kind (for rate limiting).
    last_suggestion_time: std::sync::Mutex<std::collections::HashMap<String, DateTime<Utc>>>,
    /// Whether onboarding suggestions have been generated this session.
    onboarding_generated: std::sync::Mutex<bool>,
}

impl SuggestionEngine {
    pub fn new(store: Arc<SuggestionStore>, bus: Arc<EventBus>) -> Self {
        Self {
            store,
            bus,
            recent_events: std::sync::Mutex::new(Vec::with_capacity(64)),
            last_suggestion_time: std::sync::Mutex::new(std::collections::HashMap::new()),
            onboarding_generated: std::sync::Mutex::new(false),
        }
    }

    /// Feed a system event into the engine.
    /// The engine may produce a suggestion in response.
    pub fn on_event(&self, event: &SystemEvent) {
        // Store recent event for context.
        if let Ok(mut events) = self.recent_events.lock() {
            events.push(ContextEvent {
                timestamp: event.timestamp,
                kind: format!("{:?}", event.kind),
                data: serde_json::to_value(&event.payload).unwrap_or_default(),
            });
            // Keep only last 64 events.
            while events.len() > 64 {
                events.remove(0);
            }
        }

        // Decide whether to generate suggestions on this event type.
        match &event.payload {
            EventPayload::WindowFocused { .. } => {
                self.maybe_generate("window_focus");
            }
            EventPayload::WorkspaceChanged { .. } => {
                self.maybe_generate("workspace_change");
            }
            EventPayload::SystemIdle => {
                // Idle is a good time to surface suggestions.
                self.maybe_generate("system_idle");
            }
            EventPayload::SystemActive => {
                self.maybe_generate("system_active");

                // Generate onboarding suggestions on first SystemActive event.
                let mut og = self.onboarding_generated.lock().unwrap();
                if !*og {
                    *og = true;
                    drop(og);
                    self.generate_onboarding_suggestions();
                }
            }
            _ => {}
        }
    }

    /// Rate-limited suggestion generation trigger.
    fn maybe_generate(&self, trigger: &str) {
        // Rate limit: at most 1 suggestion per 30 seconds per trigger.
        let mut last_times = self.last_suggestion_time.lock().unwrap();
        let now = Utc::now();
        let cooldown = Duration::seconds(30);

        if let Some(last) = last_times.get(trigger)
            && now.signed_duration_since(*last) < cooldown
        {
            return;
        }
        last_times.insert(trigger.to_string(), now);
        drop(last_times);

        let suggestions = self.generate_suggestions();
        for s in suggestions {
            if s.priority >= MIN_PRIORITY {
                // Check dismissal memory.
                match self.store.is_context_blocked(&s.context_hash) {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(e) => {
                        warn!("Dismissal check failed: {e}");
                        continue;
                    }
                }

                // Store the suggestion.
                if let Err(e) = self.store.insert(&s) {
                    warn!("Failed to store suggestion: {e}");
                    continue;
                }

                // Emit event for the bar.
                let event = SystemEvent::new(
                    "enad",
                    EventKind::System,
                    EventPayload::SuggestionGenerated {
                        suggestion_id: s.id,
                        kind: s.kind.as_str().to_string(),
                        title: s.title.clone(),
                        description: s.description.clone(),
                        priority: s.priority,
                        action_label: s.action.as_ref().map(|a| a.label.clone()),
                        action_type: s.action.as_ref().map(|a| a.action_type.clone()),
                        action_payload: s
                            .action
                            .as_ref()
                            .map(|a| a.payload.clone())
                            .unwrap_or(Value::Null),
                    },
                );
                self.bus.publish(event);

                info!(
                    "Ambient suggestion: {} (priority={:.2}, kind={})",
                    s.title,
                    s.priority,
                    s.kind.as_str()
                );
            }
        }
    }

    /// Build suggestions from the current context window using deterministic rules.
    /// Generate onboarding suggestions for first-time users.
    fn generate_onboarding_suggestions(&self) {
        let now = Utc::now();
        let onboarding_suggestions = vec![
            Suggestion {
                id: Uuid::new_v4(),
                kind: SuggestionKind::ContextHint,
                title: "Try: ask me anything".to_string(),
                description: "Type a command in the bar \u{2014} I can open apps, check status, and more.".to_string(),
                context_hash: "onboarding:intro".to_string(),
                priority: 0.72,
                created_at: now,
                expires_at: now + Duration::minutes(30),
                action: None,
            },
            Suggestion {
                id: Uuid::new_v4(),
                kind: SuggestionKind::ContextHint,
                title: "Workspaces are remembered".to_string(),
                description: "EnaOS remembers your workspace state. Try \u{2018}create a snapshot\u{2019} to save your current setup.".to_string(),
                context_hash: "onboarding:snapshot".to_string(),
                priority: 0.65,
                created_at: now,
                expires_at: now + Duration::minutes(30),
                action: Some(SuggestionAction {
                    label: "Try it".to_string(),
                    action_type: "take_snapshot".to_string(),
                    payload: serde_json::json!({"label": "My first snapshot"}),
                }),
            },
            Suggestion {
                id: Uuid::new_v4(),
                kind: SuggestionKind::TimeBased,
                title: "Press Escape to dismiss".to_string(),
                description: "The bar stays handy. Press Escape to collapse, type to ask.".to_string(),
                context_hash: "onboarding:escape".to_string(),
                priority: 0.58,
                created_at: now,
                expires_at: now + Duration::minutes(15),
                action: None,
            },
        ];

        let count = onboarding_suggestions.len();
        for s in onboarding_suggestions {
            if let Err(e) = self.store.insert(&s) {
                warn!("Failed to store onboarding suggestion: {e}");
                continue;
            }
            let event = SystemEvent::new(
                "enad",
                EventKind::System,
                EventPayload::SuggestionGenerated {
                    suggestion_id: s.id,
                    kind: s.kind.as_str().to_string(),
                    title: s.title.clone(),
                    description: s.description.clone(),
                    priority: s.priority,
                    action_label: s.action.as_ref().map(|a| a.label.clone()),
                    action_type: s.action.as_ref().map(|a| a.action_type.clone()),
                    action_payload: s
                        .action
                        .as_ref()
                        .map(|a| a.payload.clone())
                        .unwrap_or(Value::Null),
                },
            );
            self.bus.publish(event);
        }
        info!("Generated {} onboarding suggestions", count);
    }

    fn generate_suggestions(&self) -> Vec<Suggestion> {
        let context = self.build_context_window();
        let mut suggestions = Vec::new();
        let now = Utc::now();

        // Rule 1: Workspace continuity — if workspace recently changed.
        if let Some(s) = self.rule_workspace_continuity(&context, now) {
            suggestions.push(s);
        }

        // Rule 2: Time-based greeting.
        if let Some(s) = self.rule_time_based(&context, now) {
            suggestions.push(s);
        }

        // Rule 3: Focus-based context hint.
        if let Some(s) = self.rule_context_hint(&context, now) {
            suggestions.push(s);
        }

        suggestions
    }

    fn build_context_window(&self) -> ContextWindow {
        let events = self.recent_events.lock().unwrap();
        let now = Utc::now();

        let mut window = ContextWindow::default();

        // Extract focused app and workspace from recent events.
        for event in events.iter().rev() {
            if window.focused_app.is_empty() {
                if let Some(app) = event.data.get("app").and_then(|v| v.as_str()) {
                    window.focused_app = app.to_string();
                }
                if let Some(title) = event.data.get("title").and_then(|v| v.as_str()) {
                    window.focused_title = title.to_string();
                }
            }
            if window.workspace.is_empty()
                && let Some(ws) = event.data.get("workspace").and_then(|v| v.as_str())
            {
                window.workspace = ws.to_string();
            }
        }

        // Time labels.
        let hour = now.hour();
        window.time_label = match hour {
            5..=11 => "morning".to_string(),
            12..=16 => "afternoon".to_string(),
            17..=20 => "evening".to_string(),
            _ => "night".to_string(),
        };

        let weekday = now.format("%u").to_string(); // 1=Mon, 7=Sun
        window.day_label = match weekday.as_str() {
            "6" | "7" => "weekend".to_string(),
            _ => "weekday".to_string(),
        };

        window
    }

    /// Workspace continuity: a new workspace was focused — suggest returning or continuing.
    fn rule_workspace_continuity(
        &self,
        ctx: &ContextWindow,
        now: DateTime<Utc>,
    ) -> Option<Suggestion> {
        if ctx.workspace.is_empty() {
            return None;
        }

        let ch = format!("ws:{}", ctx.workspace);
        let priority = 0.55;

        let title = format!("Continue: {} workspace", ctx.workspace);
        let description = format!("You're on workspace {}. Keep working here?", ctx.workspace);

        Some(Suggestion {
            id: Uuid::new_v4(),
            kind: SuggestionKind::WorkspaceContinuity,
            title,
            description,
            context_hash: ch,
            priority,
            created_at: now,
            expires_at: now + Duration::minutes(DEFAULT_TTL_MINUTES),
            action: Some(SuggestionAction {
                label: "Stay".to_string(),
                action_type: "focus_workspace".to_string(),
                payload: serde_json::json!({"workspace": ctx.workspace}),
            }),
        })
    }

    /// Time-based: subtle greeting based on time of day.
    fn rule_time_based(&self, ctx: &ContextWindow, now: DateTime<Utc>) -> Option<Suggestion> {
        let ch = format!("time:{}:{}", ctx.time_label, ctx.day_label);

        // Only show once per time-of-day period.
        let priority = match (ctx.time_label.as_str(), ctx.day_label.as_str()) {
            ("morning", "weekday") => 0.45,
            ("morning", "weekend") => 0.35,
            ("afternoon", _) => 0.30,
            ("evening", _) => 0.25,
            ("night", _) => 0.20,
            _ => 0.15,
        };

        if priority < MIN_PRIORITY {
            return None;
        }

        let (title, description) = match (ctx.time_label.as_str(), ctx.day_label.as_str()) {
            ("morning", "weekday") => (
                "Good morning — ready to work?".to_string(),
                "You have a full day ahead. What's first?".to_string(),
            ),
            ("morning", "weekend") => (
                "Good morning — weekend mode.".to_string(),
                "A slower pace today. Let me know if you need anything.".to_string(),
            ),
            ("afternoon", _) => (
                "Afternoon session.".to_string(),
                "Keep the momentum going.".to_string(),
            ),
            ("evening", _) => (
                "Evening — wrapping up?".to_string(),
                "Finish strong or pick up tomorrow.".to_string(),
            ),
            ("night", _) => (
                "Late night session.".to_string(),
                "Don't forget to rest. I'll be here.".to_string(),
            ),
            _ => return None,
        };

        Some(Suggestion {
            id: Uuid::new_v4(),
            kind: SuggestionKind::TimeBased,
            title,
            description,
            context_hash: ch,
            priority,
            created_at: now,
            expires_at: now + Duration::minutes(DEFAULT_TTL_MINUTES),
            action: None,
        })
    }

    /// Context hint: based on what app the user is focused on.
    fn rule_context_hint(&self, ctx: &ContextWindow, now: DateTime<Utc>) -> Option<Suggestion> {
        if ctx.focused_app.is_empty() {
            return None;
        }

        let ch = format!("app:{}", ctx.focused_app);
        let priority = 0.50;

        let title = format!("Working in {}", ctx.focused_app);
        let description = format!(
            "You're focused on {}. Need related resources?",
            ctx.focused_app
        );

        Some(Suggestion {
            id: Uuid::new_v4(),
            kind: SuggestionKind::ContextHint,
            title,
            description,
            context_hash: ch,
            priority,
            created_at: now,
            expires_at: now + Duration::minutes(DEFAULT_TTL_MINUTES),
            action: Some(SuggestionAction {
                label: "Show resources".to_string(),
                action_type: "show_context".to_string(),
                payload: serde_json::json!({"app": ctx.focused_app}),
            }),
        })
    }

    // ── Public API methods ────────────────────────────────────

    /// Get active suggestions for the bar.
    pub fn get_suggestions(&self, limit: usize) -> Response {
        match self.store.list_active(limit) {
            Ok(suggestions) => Response::Data {
                payload: serde_json::json!({ "suggestions": suggestions }),
            },
            Err(e) => Response::Error {
                code: "SUGGESTION_LIST_FAILED".into(),
                message: e,
            },
        }
    }

    /// Dismiss a suggestion.
    pub fn dismiss_suggestion(&self, suggestion_id: &Uuid, permanent: bool) -> Response {
        // Get the suggestion details before removing.
        let (kind, context_hash) = match self.store.get(suggestion_id) {
            Ok(Some(s)) => (s.kind, s.context_hash),
            _ => {
                // Already removed — still record dismissal for the ID.
                (SuggestionKind::ContextHint, String::new())
            }
        };

        match self
            .store
            .record_dismissal(suggestion_id, &kind, &context_hash, permanent)
        {
            Ok(()) => Response::Ok {
                message: Some("Suggestion dismissed".into()),
            },
            Err(e) => Response::Error {
                code: "DISMISS_FAILED".into(),
                message: e,
            },
        }
    }

    /// Periodically clean up expired entries.
    pub fn cleanup(&self) {
        if let Ok(n) = self.store.expire()
            && n > 0
        {
            info!("Expired {n} old suggestions");
        }
        if let Ok(n) = self.store.expire_dismissals()
            && n > 0
        {
            info!("Expired {n} old dismissal records");
        }
    }
}
