use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What to include in a restoration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSelections {
    /// Restore applications (open apps).
    pub applications: bool,
    /// Restore workspace layout.
    pub workspaces: bool,
    /// Restore terminal sessions.
    pub terminals: bool,
    /// Restore browser URLs.
    pub browser_urls: bool,
    /// Restore orchestration context.
    pub orchestration_context: bool,
}

/// Preview of what a restoration will do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePreview {
    pub snapshot_id: Uuid,
    pub snapshot_label: String,
    pub snapshot_taken_at: String,
    pub action_count: u32,
    pub actions: Vec<RestoreActionPreview>,
}

/// A single action that will be performed during restoration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreActionPreview {
    pub label: String,
    pub action_type: String,
    pub target: String,
    pub requires_approval: bool,
}

/// Result of a restoration request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub snapshot_id: Uuid,
    pub plan_id: Uuid,
    pub action_count: u32,
}
