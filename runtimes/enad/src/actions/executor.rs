/// Action executor — manages action lifecycle, tracks state, emits events.
///
/// The executor:
/// 1. Validates the action request
/// 2. Checks permission level
/// 3. Executes the action handler
/// 4. Emits lifecycle events (started, completed, failed, cancelled)
/// 5. Tracks running actions for cancellation
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::actions::handlers;
use crate::actions::types::{ActionRequest, ActionStatus, ActionType, PermissionLevel};
use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

/// Tracks a running action for cancellation.
struct RunningAction {
    id: Uuid,
    // In the future, this could hold a CancellationToken.
    // For now, actions are short-lived and don't support mid-execution cancellation.
}

/// Action executor.
pub struct ActionExecutor {
    bus: Arc<EventBus>,
    running: Mutex<HashMap<Uuid, RunningAction>>,
}

impl ActionExecutor {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            bus,
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Execute an action request through the full lifecycle.
    pub async fn execute(&self, mut request: ActionRequest) -> Result<Uuid, String> {
        let action_id = request.id;

        // Set default permission if not specified.
        if request.permission == PermissionLevel::Safe
            && matches!(request.action, ActionType::LaunchCommand { .. })
        {
            request.permission = PermissionLevel::ConfirmationRequired;
        }

        // Validate permission level.
        let default_perm = ActionRequest::default_permission(&request.action);
        if matches!(request.permission, PermissionLevel::ConfirmationRequired)
            && !matches!(default_perm, PermissionLevel::ConfirmationRequired)
        {
            // Confirmation-required actions need explicit approval.
            // For now, we log and proceed — in production this would wait for user confirmation.
            warn!("Action {action_id} requires confirmation — auto-approving for now");
        }

        info!("Action {action_id}: executing {:?}", request.action);

        // Emit ActionRequested event.
        self.emit_event(action_id, ActionStatus::Pending, "Action requested");

        // Track as running.
        {
            let mut running = self.running.lock().await;
            running.insert(action_id, RunningAction { id: action_id });
        }

        // Emit ActionStarted event.
        let action_label = action_label(&request.action);
        self.emit_event(
            action_id,
            ActionStatus::Running,
            &format!("Starting: {action_label}"),
        );

        // Execute the action handler.
        let result = handlers::execute(&request.action).await;

        // Remove from running.
        {
            let mut running = self.running.lock().await;
            running.remove(&action_id);
        }

        // Emit completion event.
        match result {
            Ok(message) => {
                info!("Action {action_id}: completed — {message}");
                self.emit_event(action_id, ActionStatus::Completed, &message);
                Ok(action_id)
            }
            Err(error) => {
                warn!("Action {action_id}: failed — {error}");
                self.emit_event(
                    action_id,
                    ActionStatus::Failed {
                        error: error.clone(),
                    },
                    &error,
                );
                Err(error)
            }
        }
    }

    /// Cancel a running action.
    pub async fn cancel(&self, action_id: Uuid) -> Result<(), String> {
        let mut running = self.running.lock().await;
        if running.remove(&action_id).is_some() {
            info!("Action {action_id}: cancelled");
            self.emit_event(action_id, ActionStatus::Cancelled, "Action cancelled");
            Ok(())
        } else {
            Err(format!("No running action with id {action_id}"))
        }
    }

    /// List all running actions.
    pub async fn list_running(&self) -> Vec<Uuid> {
        self.running.lock().await.keys().cloned().collect()
    }

    /// Emit an action lifecycle event on the bus.
    fn emit_event(&self, action_id: Uuid, status: ActionStatus, message: &str) {
        let payload = match &status {
            ActionStatus::Pending => EventPayload::ActionRequested {
                action_id,
                action_type: action_type_string(&status),
                message: message.to_string(),
            },
            ActionStatus::Running => EventPayload::ActionStarted {
                action_id,
                message: message.to_string(),
            },
            ActionStatus::Completed => EventPayload::ActionCompleted {
                action_id,
                result: message.to_string(),
            },
            ActionStatus::Failed { error } => EventPayload::ActionFailed {
                action_id,
                error: error.clone(),
            },
            ActionStatus::Cancelled => EventPayload::ActionCancelled { action_id },
        };

        self.bus
            .publish(SystemEvent::new("actions", EventKind::System, payload));
    }
}

fn action_type_string(status: &ActionStatus) -> String {
    match status {
        ActionStatus::Pending => "pending".to_string(),
        ActionStatus::Running => "running".to_string(),
        ActionStatus::Completed => "completed".to_string(),
        ActionStatus::Failed { .. } => "failed".to_string(),
        ActionStatus::Cancelled => "cancelled".to_string(),
    }
}

fn action_label(action: &ActionType) -> String {
    match action {
        ActionType::OpenApp { app } => format!("Open {app}"),
        ActionType::OpenUrl { url: _ } => "Open URL".to_string(),
        ActionType::FocusWindow { app, title } => {
            format!(
                "Focus {}",
                app.as_deref().or(title.as_deref()).unwrap_or("window")
            )
        }
        ActionType::LaunchCommand { command, .. } => format!("Run {command}"),
        ActionType::SwitchWorkspace { workspace } => format!("Switch to {workspace}"),
        ActionType::SearchFiles { query, .. } => format!("Search '{query}'"),
        ActionType::MediaControl { action } => format!("Media {action}"),
        ActionType::ClipboardSet { .. } => "Set clipboard".to_string(),
        ActionType::ReadWindowTitle => "Read window title".to_string(),
        ActionType::Notify { title, .. } => format!("Notify: {title}"),
    }
}
