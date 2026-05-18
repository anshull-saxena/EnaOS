use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Top-level event on the enad event bus.
/// Every component subscribes to event types it cares about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub kind: EventKind,
    pub payload: EventPayload,
}

/// Event category — used for subscription filtering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// Agent lifecycle events (spawned, progress, terminated)
    Agent,
    /// Window/compositor events (focus, open, close, move)
    Window,
    /// Input events (keyboard shortcut, hotkey)
    Input,
    /// Process lifecycle (started, exited, crashed)
    Process,
    /// System state (idle, active, sleep, network, power)
    System,
    /// Audio/media state changes
    Audio,
    /// Clipboard events
    Clipboard,
    /// Desktop notification events
    Notification,
    /// Debug / internal
    Debug,
}

/// Structured payload per event kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum EventPayload {
    // ── Agent events ──
    AgentSpawned {
        agent_id: Uuid,
        task: String,
    },
    AgentProgress {
        agent_id: Uuid,
        progress: f32,
        message: String,
    },
    AgentCompleted {
        agent_id: Uuid,
        result: String,
    },
    AgentFailed {
        agent_id: Uuid,
        error: String,
    },

    // ── Window events ──
    WindowFocused {
        app: String,
        title: String,
    },
    WindowOpened {
        app: String,
        pid: u32,
    },
    WindowClosed {
        app: String,
        pid: u32,
    },

    // ── Workspace events ──
    WorkspaceChanged {
        workspace: String,
        output: Option<String>,
    },

    // ── Input events ──
    HotkeyPressed {
        key: String,
        modifiers: Vec<String>,
    },

    // ── Process events ──
    ProcessStarted {
        pid: u32,
        command: String,
    },
    ProcessExited {
        pid: u32,
        exit_code: i32,
    },

    // ── System events ──
    SystemIdle,
    SystemActive,
    SystemSleep,
    SystemWake,
    NetworkStatus {
        connected: bool,
        ssid: Option<String>,
        strength: Option<u8>,
    },
    BatteryStatus {
        percentage: f64,
        state: String,
        time_to_empty: Option<i64>,
        time_to_full: Option<i64>,
    },
    PowerProfileChanged {
        profile: String,
    },

    // ── Audio events ──
    AudioDeviceChanged {
        default_sink: String,
        default_source: String,
    },
    AudioVolumeChanged {
        sink_name: String,
        volume: f64,
        muted: bool,
    },
    MediaPlayback {
        player: String,
        state: String,
        title: Option<String>,
        artist: Option<String>,
    },

    // ── Clipboard events ──
    ClipboardUpdated {
        content_type: String,
        preview: String,
    },

    // ── Notification events ──
    NotificationReceived {
        id: u32,
        app_name: String,
        summary: String,
        body: Option<String>,
        urgency: String,
    },
    NotificationDismissed {
        id: u32,
    },

    // ── Action execution events ──
    ActionRequested {
        action_id: Uuid,
        action_type: String,
        message: String,
    },
    ActionStarted {
        action_id: Uuid,
        message: String,
    },
    ActionCompleted {
        action_id: Uuid,
        result: String,
    },
    ActionFailed {
        action_id: Uuid,
        error: String,
    },
    ActionCancelled {
        action_id: Uuid,
    },

    // ── Orchestration events ──
    OrchestrationPlanEvent {
        plan_id: Uuid,
        status: String,
        message: String,
    },
    OrchestrationNodeEvent {
        plan_id: Uuid,
        node_id: Uuid,
        status: String,
        label: String,
        error: Option<String>,
        result: Option<String>,
    },

    // ── Debug ──
    Log {
        level: String,
        message: String,
    },
}

impl SystemEvent {
    pub fn new(source: &str, kind: EventKind, payload: EventPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            source: source.to_string(),
            kind,
            payload,
        }
    }
}
