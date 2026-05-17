use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Permission level required for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    /// Safe to execute without confirmation.
    Safe,
    /// Requires logging but auto-executes.
    Privileged,
    /// Requires explicit user confirmation before execution.
    ConfirmationRequired,
}

/// Status of an action execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStatus {
    Pending,
    Running,
    Completed,
    Failed { error: String },
    Cancelled,
}

/// A desktop action request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub id: Uuid,
    pub action: ActionType,
    pub params: serde_json::Value,
    pub permission: PermissionLevel,
}

/// Supported desktop action types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "params")]
pub enum ActionType {
    /// Open an application by name.
    OpenApp { app: String },
    /// Open a URL in the default browser.
    OpenUrl { url: String },
    /// Focus a window by app name or title substring.
    FocusWindow { app: Option<String>, title: Option<String> },
    /// Launch a terminal command (supervised).
    LaunchCommand { command: String, args: Vec<String> },
    /// Switch to a workspace.
    SwitchWorkspace { workspace: String },
    /// Search for files by name pattern.
    SearchFiles { query: String, path: Option<String> },
    /// Control media playback (play/pause/next/previous).
    MediaControl { action: String },
    /// Set clipboard content.
    ClipboardSet { text: String },
    /// Read the active window title (returns data, no side effect).
    ReadWindowTitle,
    /// Show a desktop notification.
    Notify { title: String, body: String },
}

impl ActionRequest {
    pub fn new(action: ActionType, permission: PermissionLevel) -> Self {
        Self {
            id: Uuid::new_v4(),
            action,
            params: serde_json::json!({}),
            permission,
        }
    }

    /// Determine the permission level for an action type.
    pub fn default_permission(action: &ActionType) -> PermissionLevel {
        match action {
            ActionType::ReadWindowTitle => PermissionLevel::Safe,
            ActionType::OpenApp { .. } => PermissionLevel::Safe,
            ActionType::OpenUrl { .. } => PermissionLevel::Safe,
            ActionType::MediaControl { .. } => PermissionLevel::Safe,
            ActionType::Notify { .. } => PermissionLevel::Safe,
            ActionType::FocusWindow { .. } => PermissionLevel::Privileged,
            ActionType::SwitchWorkspace { .. } => PermissionLevel::Privileged,
            ActionType::ClipboardSet { .. } => PermissionLevel::Privileged,
            ActionType::SearchFiles { .. } => PermissionLevel::Privileged,
            ActionType::LaunchCommand { .. } => PermissionLevel::ConfirmationRequired,
        }
    }
}
