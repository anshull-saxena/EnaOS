use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A complete workspace snapshot — the environment state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub snapshot_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub label: String,
    pub workspaces: Vec<WorkspaceInfo>,
    pub applications: Vec<AppInfo>,
    pub windows: Vec<WindowInfo>,
    pub terminals: Vec<TerminalInfo>,
    pub browser_urls: Vec<BrowserTab>,
    pub orchestration_plans: Vec<OrchestrationPlanRef>,
    pub recent_actions: Vec<ActionRef>,
    pub ai_conversations: Vec<ConversationRef>,
    pub active_project: Option<String>,
    pub context_summary: Option<String>,
    pub is_auto: bool,
    pub env_checksum: String,
}

impl WorkspaceSnapshot {
    pub fn new(label: &str) -> Self {
        Self {
            snapshot_id: Uuid::new_v4(),
            created_at: Utc::now(),
            label: label.to_string(),
            workspaces: Vec::new(),
            applications: Vec::new(),
            windows: Vec::new(),
            terminals: Vec::new(),
            browser_urls: Vec::new(),
            orchestration_plans: Vec::new(),
            recent_actions: Vec::new(),
            ai_conversations: Vec::new(),
            active_project: None,
            context_summary: None,
            is_auto: true,
            env_checksum: String::new(),
        }
    }

    /// Number of actionable items in this snapshot (for restoration planning).
    pub fn node_count(&self) -> u32 {
        let mut count = 0u32;
        if !self.workspaces.is_empty() {
            count += 1;
        }
        count += self.applications.len() as u32;
        count += self.terminals.len() as u32;
        count
    }
}

/// A workspace (virtual desktop) that was active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub name: String,
    pub output: Option<String>,
    pub is_focused: bool,
}

/// A running application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub title: String,
    pub pid: Option<u32>,
    pub is_focused: bool,
}

/// A specific window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub app: String,
    pub title: String,
    pub pid: Option<u32>,
    pub workspace: Option<String>,
    pub geometry: Option<String>,
}

/// A terminal session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub app: String,
    pub pid: Option<u32>,
    pub working_directory: Option<String>,
    pub command: Option<String>,
}

/// A browser tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTab {
    pub browser: String,
    pub url: String,
    pub title: Option<String>,
}

/// Reference to an orchestration plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationPlanRef {
    pub plan_id: Uuid,
    pub title: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Reference to a recent action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRef {
    pub action_id: Uuid,
    pub action_type: String,
    pub summary: String,
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

/// Reference to an AI conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRef {
    pub query: String,
    pub response_summary: String,
    pub timestamp: DateTime<Utc>,
}

/// Summary info for list view (no heavy JSON blobs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub snapshot_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub label: String,
    pub is_auto: bool,
    pub app_count: usize,
    pub terminal_count: usize,
    pub plan_count: usize,
    pub is_restored: bool,
}
