use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Categories of ambient suggestions the system can produce.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SuggestionKind {
    /// Continue a previous workspace session.
    WorkspaceContinuity,
    /// Resume a recent task or project.
    TaskResurfacing,
    /// Environment-aware hint (e.g., "Wayland docs open in another workspace").
    ContextHint,
    /// Reopen a recently modified document.
    RecentDocument,
    /// Resume a terminal session with running processes.
    TerminalSession,
    /// Time-based suggestion (e.g., "Morning — start your session?").
    TimeBased,
}

impl SuggestionKind {
    /// Default cooldown in minutes before this kind can resurface after dismissal.
    pub fn cooldown_minutes(&self) -> u32 {
        match self {
            SuggestionKind::WorkspaceContinuity => 30,
            SuggestionKind::TaskResurfacing => 15,
            SuggestionKind::ContextHint => 10,
            SuggestionKind::RecentDocument => 20,
            SuggestionKind::TerminalSession => 30,
            SuggestionKind::TimeBased => 60,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SuggestionKind::WorkspaceContinuity => "workspace_continuity",
            SuggestionKind::TaskResurfacing => "task_resurfacing",
            SuggestionKind::ContextHint => "context_hint",
            SuggestionKind::RecentDocument => "recent_document",
            SuggestionKind::TerminalSession => "terminal_session",
            SuggestionKind::TimeBased => "time_based",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "workspace_continuity" => Some(SuggestionKind::WorkspaceContinuity),
            "task_resurfacing" => Some(SuggestionKind::TaskResurfacing),
            "context_hint" => Some(SuggestionKind::ContextHint),
            "recent_document" => Some(SuggestionKind::RecentDocument),
            "terminal_session" => Some(SuggestionKind::TerminalSession),
            "time_based" => Some(SuggestionKind::TimeBased),
            _ => None,
        }
    }
}

/// An actionable suggestion that the ambient layer can surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: Uuid,
    pub kind: SuggestionKind,
    /// Short user-facing label (e.g., "Continue: EnaOS Development").
    pub title: String,
    /// Longer description shown on hover or as subtitle.
    pub description: String,
    /// Hash of the context that produced this suggestion (for dedup).
    pub context_hash: String,
    /// Relevance score 0.0–1.0 (set by engine, consumed by UI for priority).
    pub priority: f64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Optional action the user can take.
    pub action: Option<SuggestionAction>,
}

/// An action the user can take on a suggestion (one-click).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionAction {
    pub label: String,
    pub action_type: String,
    pub payload: serde_json::Value,
}

/// Lightweight summary for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionSummary {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub priority: f64,
    pub created_at: DateTime<Utc>,
    pub action_label: Option<String>,
    pub action_type: Option<String>,
}

impl From<Suggestion> for SuggestionSummary {
    fn from(s: Suggestion) -> Self {
        SuggestionSummary {
            id: s.id,
            kind: s.kind.as_str().to_string(),
            title: s.title,
            description: s.description,
            priority: s.priority,
            created_at: s.created_at,
            action_label: s.action.as_ref().map(|a| a.label.clone()),
            action_type: s.action.as_ref().map(|a| a.action_type.clone()),
        }
    }
}

/// Context window snapshot used by the suggestion engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextWindow {
    pub focused_app: String,
    pub focused_title: String,
    pub workspace: String,
    pub recent_events: Vec<ContextEvent>,
    pub idle_duration_secs: u64,
    pub time_label: String, // "morning", "afternoon", "evening", "night"
    pub day_label: String,  // "weekday", "weekend"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEvent {
    pub timestamp: DateTime<Utc>,
    pub kind: String,
    pub data: serde_json::Value,
}

/// Dismissal record for memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DismissalRecord {
    pub suggestion_id: Uuid,
    pub kind: String,
    pub context_hash: String,
    pub dismissed_at: DateTime<Utc>,
    pub cooldown_until: DateTime<Utc>,
    pub permanent: bool,
}
