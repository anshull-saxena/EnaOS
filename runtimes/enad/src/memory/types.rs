/// Memory entry types and schemas.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A memory entry stored in the working memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub entry_type: MemoryType,
    pub workspace: Option<String>,
    pub summary: String,
    pub details: serde_json::Value,
    pub relevance: f32,
}

/// Types of memory entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    /// System event (window focus, workspace change, battery, etc.)
    Event,
    /// Desktop action execution
    Action,
    /// Full desktop context snapshot
    ContextSnapshot,
    /// Workspace state snapshot
    WorkspaceSnapshot,
    /// User intent (query to AI runtime)
    Intent,
    /// AI response summary
    AiResponse,
    /// Auto-generated memory summary
    Summary,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Event => write!(f, "event"),
            MemoryType::Action => write!(f, "action"),
            MemoryType::ContextSnapshot => write!(f, "context_snapshot"),
            MemoryType::WorkspaceSnapshot => write!(f, "workspace_snapshot"),
            MemoryType::Intent => write!(f, "intent"),
            MemoryType::AiResponse => write!(f, "ai_response"),
            MemoryType::Summary => write!(f, "summary"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "event" => Ok(MemoryType::Event),
            "action" => Ok(MemoryType::Action),
            "context_snapshot" => Ok(MemoryType::ContextSnapshot),
            "workspace_snapshot" => Ok(MemoryType::WorkspaceSnapshot),
            "intent" => Ok(MemoryType::Intent),
            "ai_response" => Ok(MemoryType::AiResponse),
            "summary" => Ok(MemoryType::Summary),
            _ => Err(format!("Unknown memory type: {s}")),
        }
    }
}

/// Query parameters for memory retrieval.
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    pub entry_types: Vec<MemoryType>,
    pub workspace: Option<String>,
    pub limit: usize,
    pub since: Option<DateTime<Utc>>,
    pub search: Option<String>,
}

impl MemoryQuery {
    pub fn new() -> Self {
        Self {
            limit: 20,
            ..Default::default()
        }
    }

    pub fn actions() -> Self {
        Self {
            entry_types: vec![MemoryType::Action],
            limit: 20,
            ..Default::default()
        }
    }

    pub fn intents() -> Self {
        Self {
            entry_types: vec![MemoryType::Intent, MemoryType::AiResponse],
            limit: 10,
            ..Default::default()
        }
    }

    pub fn recent_context() -> Self {
        Self {
            entry_types: vec![MemoryType::ContextSnapshot, MemoryType::WorkspaceSnapshot],
            limit: 5,
            ..Default::default()
        }
    }

    pub fn for_workspace(workspace: &str) -> Self {
        Self {
            workspace: Some(workspace.to_string()),
            limit: 30,
            ..Default::default()
        }
    }

    pub fn search(query: &str) -> Self {
        Self {
            search: Some(query.to_string()),
            limit: 15,
            ..Default::default()
        }
    }
}

/// Summary of current working memory state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
    pub total_entries: i64,
    pub oldest_entry: Option<DateTime<Utc>>,
    pub newest_entry: Option<DateTime<Utc>>,
    pub entry_counts: serde_json::Value,
    pub workspaces: Vec<String>,
    pub recent_intents: Vec<String>,
    pub recent_actions: Vec<String>,
    pub current_context: serde_json::Value,
}
