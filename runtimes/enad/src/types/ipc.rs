use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message envelope for all IPC between Ena Bar ↔ enad.
/// Each message has an ID for request-response correlation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: Uuid,
    pub kind: MessageKind,
}

/// Top-level message discrimination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "body")]
pub enum MessageKind {
    // ── Commands (Bar → Daemon) ──
    Command(Command),

    // ── Responses (Daemon → Bar) ──
    Response(Response),

    // ── Event subscriptions ──
    Subscribe(Subscription),

    // ── Pushed events (Daemon → Bar) ──
    Event(super::events::SystemEvent),

    // ── Heartbeat / Ping ──
    Ping,
    Pong,
}

/// Commands the Ena Bar sends to enad.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Execute a system-level command
    Execute {
        command: String,
        args: Vec<String>,
    },
    /// Execute a desktop action (open app, focus window, etc.)
    ExecuteAction {
        action: String,
        params: serde_json::Value,
    },
    /// Cancel a running action.
    CancelAction {
        action_id: Uuid,
    },
    /// Spawn an agent with a task description
    SpawnAgent {
        task: String,
        capabilities: Vec<String>,
    },
    /// Query current system state
    QueryState {
        target: StateTarget,
    },
    /// Terminate a running process/agent
    Terminate {
        id: Uuid,
    },
    /// Get system context for AI prompts
    GetContext,

    // ── Orchestration commands ──
    /// Submit an execution plan.
    SubmitPlan {
        plan: serde_json::Value,
    },
    /// Approve a pending plan.
    ApprovePlan {
        plan_id: Uuid,
    },
    /// Reject a pending plan.
    RejectPlan {
        plan_id: Uuid,
    },
    /// Cancel a running plan.
    CancelPlan {
        plan_id: Uuid,
    },
    /// List all plans.
    ListPlans,

    // ── Workspace Snapshot commands ──
    /// Take a workspace snapshot.
    TakeSnapshot {
        label: Option<String>,
    },
    /// List recent snapshots.
    ListSnapshots {
        limit: Option<u32>,
    },
    /// Get a full snapshot by ID.
    GetSnapshot {
        snapshot_id: Uuid,
    },
    /// Delete a snapshot.
    DeleteSnapshot {
        snapshot_id: Uuid,
    },

    // ── Restoration commands ──
    /// Preview what a snapshot restoration would do.
    PreviewRestore {
        snapshot_id: Uuid,
    },
    /// Restore a workspace snapshot as an orchestration plan.
    RestoreSnapshot {
        snapshot_id: Uuid,
        selections: Option<serde_json::Value>,
    },

    // ── Ambient suggestion commands ──
    /// Get active suggestions.
    GetSuggestions {
        limit: Option<u32>,
    },
    /// Dismiss a suggestion.
    DismissSuggestion {
        suggestion_id: Uuid,
        permanent: Option<bool>,
    },

    // ── Contextual Command Intelligence ──
    /// Get context-aware command suggestions.
    GetContextCommands {
        query: String,
        limit: Option<u32>,
    },

    // ── First-Run / Onboarding commands ──
    /// Get first-run status (fresh install, onboarding completed, etc.).
    GetFirstRunStatus,
    /// Mark onboarding as completed (welcome overlay dismissed).
    CompleteOnboarding,
    /// Get demo data for fresh installs.
    GetDemoData,
}

/// What state to query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateTarget {
    ActiveWindows,
    RunningAgents,
    SystemInfo,
    ProcessList,
    /// Full desktop context (battery, network, audio, focused window, workspace).
    DesktopContext,
    /// Recent memory entries.
    MemoryRecent,
    /// Memory summary.
    MemorySummary,
    /// Memory search.
    MemorySearch { query: String },
}

/// Responses from enad back to the Ena Bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    /// Command executed successfully
    Ok {
        message: Option<String>,
    },
    /// Data payload returned
    Data {
        payload: serde_json::Value,
    },
    /// Something went wrong
    Error {
        code: String,
        message: String,
    },
}

/// Event subscription request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Which event kinds to subscribe to. Empty = all.
    pub kinds: Vec<super::events::EventKind>,
}

impl IpcMessage {
    pub fn command(command: Command) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: MessageKind::Command(command),
        }
    }

    pub fn response(id: Uuid, response: Response) -> Self {
        Self {
            id,
            kind: MessageKind::Response(response),
        }
    }

    pub fn event(event: super::events::SystemEvent) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: MessageKind::Event(event),
        }
    }

    pub fn subscribe(kinds: Vec<super::events::EventKind>) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: MessageKind::Subscribe(Subscription { kinds }),
        }
    }

    pub fn ping() -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: MessageKind::Ping,
        }
    }

    pub fn pong() -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: MessageKind::Pong,
        }
    }
}
