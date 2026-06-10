use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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

    // ── Workspace Snapshot events ──
    SnapshotTaken {
        snapshot_id: Uuid,
        label: String,
        node_count: u32,
    },
    SnapshotDeleted {
        snapshot_id: Uuid,
    },

    // ── Restoration events ──
    RestorePreviewGenerated {
        snapshot_id: Uuid,
        plan_id: Uuid,
        action_count: u32,
    },
    RestoreStarted {
        snapshot_id: Uuid,
        plan_id: Uuid,
        description: String,
    },

    // ── Ambient suggestions ──
    SuggestionGenerated {
        suggestion_id: Uuid,
        kind: String,
        title: String,
        description: String,
        priority: f64,
        action_label: Option<String>,
        action_type: Option<String>,
        action_payload: Value,
    },
    SuggestionDismissed { suggestion_id: Uuid, reason: String },

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

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_event_kind(kind: EventKind) {
        let event = SystemEvent::new("test", kind.clone(), EventPayload::SystemActive);
        let json = serde_json::to_value(&event).unwrap();
        let parsed: SystemEvent = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.kind, kind);
    }

    fn roundtrip_payload(payload: EventPayload) {
        let event = SystemEvent::new("test", EventKind::System, payload);
        let json = serde_json::to_value(&event).unwrap();
        let parsed: SystemEvent = serde_json::from_value(json).unwrap();
        // Verify the payload variant matches by re-serializing to JSON.
        let expected = serde_json::to_value(&event.payload).unwrap();
        let actual = serde_json::to_value(&parsed.payload).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_event_kinds_all_roundtrip() {
        let kinds = vec![
            EventKind::Agent,
            EventKind::Window,
            EventKind::Input,
            EventKind::Process,
            EventKind::System,
            EventKind::Audio,
            EventKind::Clipboard,
            EventKind::Notification,
            EventKind::Debug,
        ];
        for kind in kinds {
            roundtrip_event_kind(kind);
        }
    }

    #[test]
    fn test_payload_unit_variants() {
        roundtrip_payload(EventPayload::SystemIdle);
        roundtrip_payload(EventPayload::SystemActive);
        roundtrip_payload(EventPayload::SystemSleep);
        roundtrip_payload(EventPayload::SystemWake);
    }

    #[test]
    fn test_payload_window_focused() {
        roundtrip_payload(EventPayload::WindowFocused {
            app: "Alacritty".into(),
            title: "~/projects".into(),
        });
    }

    #[test]
    fn test_payload_window_opened() {
        roundtrip_payload(EventPayload::WindowOpened {
            app: "Firefox".into(),
            pid: 1234,
        });
    }

    #[test]
    fn test_payload_window_closed() {
        roundtrip_payload(EventPayload::WindowClosed {
            app: "Firefox".into(),
            pid: 1234,
        });
    }

    #[test]
    fn test_payload_workspace_changed() {
        roundtrip_payload(EventPayload::WorkspaceChanged {
            workspace: "2".into(),
            output: Some("HDMI-1".into()),
        });
        roundtrip_payload(EventPayload::WorkspaceChanged {
            workspace: "1".into(),
            output: None,
        });
    }

    #[test]
    fn test_payload_network() {
        roundtrip_payload(EventPayload::NetworkStatus {
            connected: true,
            ssid: Some("Home".into()),
            strength: Some(85),
        });
        roundtrip_payload(EventPayload::NetworkStatus {
            connected: false,
            ssid: None,
            strength: None,
        });
    }

    #[test]
    fn test_payload_battery() {
        roundtrip_payload(EventPayload::BatteryStatus {
            percentage: 85.5,
            state: "Discharging".into(),
            time_to_empty: Some(7200),
            time_to_full: None,
        });
    }

    #[test]
    fn test_payload_audio_volume() {
        roundtrip_payload(EventPayload::AudioVolumeChanged {
            sink_name: "alsa_output.pci-0000_00_1f.3.analog-stereo".into(),
            volume: 0.75,
            muted: false,
        });
    }

    #[test]
    fn test_payload_media_playback() {
        roundtrip_payload(EventPayload::MediaPlayback {
            player: "spotify".into(),
            state: "Playing".into(),
            title: Some("Bohemian Rhapsody".into()),
            artist: Some("Queen".into()),
        });
    }

    #[test]
    fn test_payload_notification_received() {
        roundtrip_payload(EventPayload::NotificationReceived {
            id: 42,
            app_name: "Slack".into(),
            summary: "New message".into(),
            body: Some("Hello from team".into()),
            urgency: "normal".into(),
        });
    }

    #[test]
    fn test_payload_action_events() {
        let aid = Uuid::new_v4();
        roundtrip_payload(EventPayload::ActionRequested {
            action_id: aid,
            action_type: "open_app".into(),
            message: "Open Firefox".into(),
        });
        roundtrip_payload(EventPayload::ActionStarted {
            action_id: aid,
            message: "Opening Firefox...".into(),
        });
        roundtrip_payload(EventPayload::ActionCompleted {
            action_id: aid,
            result: "done".into(),
        });
        roundtrip_payload(EventPayload::ActionFailed {
            action_id: aid,
            error: "App not found".into(),
        });
        roundtrip_payload(EventPayload::ActionCancelled { action_id: aid });
    }

    #[test]
    fn test_payload_snapshot_events() {
        let sid = Uuid::new_v4();
        roundtrip_payload(EventPayload::SnapshotTaken {
            snapshot_id: sid,
            label: "Work setup".into(),
            node_count: 5,
        });
        roundtrip_payload(EventPayload::SnapshotDeleted { snapshot_id: sid });
    }

    #[test]
    fn test_payload_restore_events() {
        roundtrip_payload(EventPayload::RestorePreviewGenerated {
            snapshot_id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            action_count: 3,
        });
        roundtrip_payload(EventPayload::RestoreStarted {
            snapshot_id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            description: "Restoring development workspace".into(),
        });
    }

    #[test]
    fn test_payload_suggestion_generated() {
        roundtrip_payload(EventPayload::SuggestionGenerated {
            suggestion_id: Uuid::new_v4(),
            kind: "context_hint".into(),
            title: "Try: ask me anything".into(),
            description: "Type a command in the bar".into(),
            priority: 0.72,
            action_label: Some("Try it".into()),
            action_type: Some("take_snapshot".into()),
            action_payload: json!({"label": "My first snapshot"}),
        });
    }

    #[test]
    fn test_payload_suggestion_dismissed() {
        roundtrip_payload(EventPayload::SuggestionDismissed {
            suggestion_id: Uuid::new_v4(),
            reason: "user_dismissed".into(),
        });
    }

    #[test]
    fn test_payload_orchestration_plan() {
        roundtrip_payload(EventPayload::OrchestrationPlanEvent {
            plan_id: Uuid::new_v4(),
            status: "Running".into(),
            message: "Plan execution started".into(),
        });
    }

    #[test]
    fn test_payload_orchestration_node() {
        roundtrip_payload(EventPayload::OrchestrationNodeEvent {
            plan_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            status: "Completed".into(),
            label: "Open Firefox".into(),
            error: None,
            result: Some("done".into()),
        });
    }

    #[test]
    fn test_system_event_roundtrip() {
        let event = SystemEvent {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            timestamp: DateTime::parse_from_rfc3339("2025-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            source: "enad".into(),
            kind: EventKind::Window,
            payload: EventPayload::WindowFocused {
                app: "Alacritty".into(),
                title: "~".into(),
            },
        };

        let json = serde_json::to_value(&event).unwrap();
        let parsed: SystemEvent = serde_json::from_value(json).unwrap();

        assert_eq!(parsed.id, event.id);
        assert_eq!(parsed.source, event.source);
        assert_eq!(parsed.kind, event.kind);
    }

    #[test]
    fn test_event_wire_format() {
        // Verify the exact wire format the bar receives on the event stream.
        let payload = EventPayload::WindowFocused {
            app: "Alacritty".into(),
            title: "~".into(),
        };
        let json = serde_json::to_value(&payload).unwrap();

        // The payload must use adjacently tagged format:
        // { "type": "WindowFocused", "data": { "app": "Alacritty", "title": "~" } }
        assert!(json.get("type").is_some(), "EventPayload must have 'type' discriminator");
        assert_eq!(json.get("type").unwrap(), "WindowFocused");
        assert!(json.get("data").is_some(), "EventPayload must have 'data' content");

        // Verify the bar's parse_event navigation works:
        assert_eq!(json.get("type").unwrap().as_str().unwrap(), "WindowFocused");
    }

    #[test]
    fn test_full_event_envelope_wire_format() {
        // Verify the exact full envelope the bar receives.
        // This matches enad's server.rs dispatch which wraps SystemEvent in IpcMessage.
        use crate::types::ipc::IpcMessage;

        let event = SystemEvent::new(
            "enad",
            EventKind::Window,
            EventPayload::WindowFocused {
                app: "Alacritty".into(),
                title: "~".into(),
            },
        );

        let msg = IpcMessage::event(event);
        let json = serde_json::to_value(&msg).unwrap();

        // Verify the bar's parse_event navigation path:
        // json["kind"]["type"] == "Event"
        // json["kind"]["body"]["kind"] == "Window"
        // json["kind"]["body"]["payload"] has adjacently tagged EventPayload
        let kind = json.get("kind").unwrap();
        assert_eq!(kind.get("type").unwrap(), "Event");
        let body = kind.get("body").unwrap();
        assert_eq!(body.get("source").unwrap(), "enad");
        assert_eq!(body.get("kind").unwrap(), "Window");
        let payload = body.get("payload").unwrap();
        assert_eq!(payload.get("type").unwrap(), "WindowFocused");
        assert_eq!(
            payload.get("data").unwrap().get("app").unwrap(),
            "Alacritty"
        );
    }
}
