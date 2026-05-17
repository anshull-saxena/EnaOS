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
