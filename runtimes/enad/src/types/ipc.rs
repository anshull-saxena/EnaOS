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
    Execute { command: String, args: Vec<String> },
    /// Execute a desktop action (open app, focus window, etc.)
    ExecuteAction {
        action: String,
        params: serde_json::Value,
    },
    /// Cancel a running action.
    CancelAction { action_id: Uuid },
    /// Spawn an agent with a task description
    SpawnAgent {
        task: String,
        capabilities: Vec<String>,
    },
    /// Query current system state
    QueryState { target: StateTarget },
    /// Terminate a running process/agent
    Terminate { id: Uuid },
    /// Get system context for AI prompts
    GetContext,

    // ── Orchestration commands ──
    /// Submit an execution plan.
    SubmitPlan { plan: serde_json::Value },
    /// Approve a pending plan.
    ApprovePlan { plan_id: Uuid },
    /// Reject a pending plan.
    RejectPlan { plan_id: Uuid },
    /// Cancel a running plan.
    CancelPlan { plan_id: Uuid },
    /// List all plans.
    ListPlans,

    // ── Workspace Snapshot commands ──
    /// Take a workspace snapshot.
    TakeSnapshot { label: Option<String> },
    /// List recent snapshots.
    ListSnapshots { limit: Option<u32> },
    /// Get a full snapshot by ID.
    GetSnapshot { snapshot_id: Uuid },
    /// Delete a snapshot.
    DeleteSnapshot { snapshot_id: Uuid },

    // ── Restoration commands ──
    /// Preview what a snapshot restoration would do.
    PreviewRestore { snapshot_id: Uuid },
    /// Restore a workspace snapshot as an orchestration plan.
    RestoreSnapshot {
        snapshot_id: Uuid,
        selections: Option<serde_json::Value>,
    },

    // ── Ambient suggestion commands ──
    /// Get active suggestions.
    GetSuggestions { limit: Option<u32> },
    /// Dismiss a suggestion.
    DismissSuggestion {
        suggestion_id: Uuid,
        permanent: Option<bool>,
    },

    // ── Contextual Command Intelligence ──
    /// Get context-aware command suggestions.
    GetContextCommands { query: String, limit: Option<u32> },

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
    MemorySearch {
        query: String,
    },
}

/// Responses from enad back to the Ena Bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    /// Command executed successfully
    Ok { message: Option<String> },
    /// Data payload returned
    Data { payload: serde_json::Value },
    /// Something went wrong
    Error { code: String, message: String },
}

/// Event subscription request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Which event kinds to subscribe to. Empty = all.
    pub kinds: Vec<super::events::EventKind>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Command round-trip tests ───────────────────────────────
    //
    // Verifies every Command variant serializes and deserializes
    // correctly through the IpcMessage envelope.

    fn roundtrip_command(cmd: Command) {
        let msg = IpcMessage::command(cmd);
        let json = serde_json::to_string(&msg).expect("serialize command");
        let parsed: IpcMessage = serde_json::from_str(&json).expect("deserialize command");
        assert_eq!(msg.id, parsed.id);
        match (&msg.kind, &parsed.kind) {
            (MessageKind::Command(a), MessageKind::Command(b)) => {
                assert_eq!(format!("{:?}", a), format!("{:?}", b));
            }
            _ => panic!("Expected Command variant"),
        }
    }

    fn roundtrip_response(resp: Response) {
        let msg = IpcMessage::response(Uuid::new_v4(), resp);
        let json = serde_json::to_string(&msg).expect("serialize response");
        let parsed: IpcMessage = serde_json::from_str(&json).expect("deserialize response");
        match (&parsed.kind, &msg.kind) {
            (MessageKind::Response(a), MessageKind::Response(b)) => {
                assert_eq!(format!("{:?}", a), format!("{:?}", b));
            }
            _ => panic!("Expected Response variant"),
        }
    }

    // All tests must use explicit assertions rather than PartialEq
    // because IpcMessage intentionally doesn't derive PartialEq.

    #[test]
    fn test_cmd_execute() {
        roundtrip_command(Command::Execute {
            command: "open".into(),
            args: vec!["brave-browser".into()],
        });
    }

    #[test]
    fn test_cmd_execute_action() {
        roundtrip_command(Command::ExecuteAction {
            action: "open_app".into(),
            params: serde_json::json!({"app": "Firefox"}),
        });
    }

    #[test]
    fn test_cmd_cancel_action() {
        roundtrip_command(Command::CancelAction {
            action_id: Uuid::new_v4(),
        });
    }

    #[test]
    fn test_cmd_spawn_agent() {
        roundtrip_command(Command::SpawnAgent {
            task: "check weather".into(),
            capabilities: vec!["web_search".into()],
        });
    }

    #[test]
    fn test_cmd_query_state() {
        roundtrip_command(Command::QueryState {
            target: StateTarget::DesktopContext,
        });
        roundtrip_command(Command::QueryState {
            target: StateTarget::ActiveWindows,
        });
        roundtrip_command(Command::QueryState {
            target: StateTarget::MemorySearch {
                query: "foo".into(),
            },
        });
    }

    #[test]
    fn test_cmd_terminate() {
        roundtrip_command(Command::Terminate { id: Uuid::new_v4() });
    }

    #[test]
    fn test_cmd_get_context() {
        roundtrip_command(Command::GetContext);
    }

    #[test]
    fn test_cmd_submit_plan() {
        roundtrip_command(Command::SubmitPlan {
            plan: serde_json::json!({"title": "test"}),
        });
    }

    #[test]
    fn test_cmd_approve_reject_cancel_plan() {
        let pid = Uuid::new_v4();
        roundtrip_command(Command::ApprovePlan { plan_id: pid });
        roundtrip_command(Command::RejectPlan { plan_id: pid });
        roundtrip_command(Command::CancelPlan { plan_id: pid });
    }

    #[test]
    fn test_cmd_list_plans() {
        roundtrip_command(Command::ListPlans);
    }

    #[test]
    fn test_cmd_take_snapshot() {
        roundtrip_command(Command::TakeSnapshot {
            label: Some("test".into()),
        });
        roundtrip_command(Command::TakeSnapshot { label: None });
    }

    #[test]
    fn test_cmd_list_snapshots() {
        roundtrip_command(Command::ListSnapshots { limit: Some(10) });
        roundtrip_command(Command::ListSnapshots { limit: None });
    }

    #[test]
    fn test_cmd_get_snapshot() {
        roundtrip_command(Command::GetSnapshot {
            snapshot_id: Uuid::new_v4(),
        });
    }

    #[test]
    fn test_cmd_delete_snapshot() {
        roundtrip_command(Command::DeleteSnapshot {
            snapshot_id: Uuid::new_v4(),
        });
    }

    #[test]
    fn test_cmd_preview_restore() {
        roundtrip_command(Command::PreviewRestore {
            snapshot_id: Uuid::new_v4(),
        });
    }

    #[test]
    fn test_cmd_restore_snapshot() {
        roundtrip_command(Command::RestoreSnapshot {
            snapshot_id: Uuid::new_v4(),
            selections: None,
        });
        roundtrip_command(Command::RestoreSnapshot {
            snapshot_id: Uuid::new_v4(),
            selections: Some(serde_json::json!({"applications": true})),
        });
    }

    #[test]
    fn test_cmd_get_suggestions() {
        roundtrip_command(Command::GetSuggestions { limit: Some(5) });
        roundtrip_command(Command::GetSuggestions { limit: None });
    }

    #[test]
    fn test_cmd_dismiss_suggestion() {
        roundtrip_command(Command::DismissSuggestion {
            suggestion_id: Uuid::new_v4(),
            permanent: Some(true),
        });
        roundtrip_command(Command::DismissSuggestion {
            suggestion_id: Uuid::new_v4(),
            permanent: None,
        });
    }

    #[test]
    fn test_cmd_get_context_commands() {
        roundtrip_command(Command::GetContextCommands {
            query: "open browser".into(),
            limit: Some(6),
        });
    }

    #[test]
    fn test_cmd_first_run() {
        roundtrip_command(Command::GetFirstRunStatus);
        roundtrip_command(Command::CompleteOnboarding);
        roundtrip_command(Command::GetDemoData);
    }

    // ── Response round-trip tests ──────────────────────────────

    #[test]
    fn test_resp_ok() {
        roundtrip_response(Response::Ok {
            message: Some("done".into()),
        });
        roundtrip_response(Response::Ok { message: None });
    }

    #[test]
    fn test_resp_data() {
        roundtrip_response(Response::Data {
            payload: serde_json::json!({"commands": [], "context": {}}),
        });
        roundtrip_response(Response::Data {
            payload: serde_json::json!([{"id": "a"}, {"id": "b"}]),
        });
        roundtrip_response(Response::Data {
            payload: serde_json::json!("simple string"),
        });
    }

    #[test]
    fn test_resp_error() {
        roundtrip_response(Response::Error {
            code: "PARSE_ERROR".into(),
            message: "Invalid JSON".into(),
        });
    }

    // ── IpcMessage envelope tests ──────────────────────────────

    #[test]
    fn test_message_ping_pong() {
        let ping = IpcMessage::ping();
        let json = serde_json::to_string(&ping).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.kind, MessageKind::Ping));

        let pong = IpcMessage::pong();
        let json = serde_json::to_string(&pong).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.kind, MessageKind::Pong));
    }

    #[test]
    fn test_message_subscribe() {
        use crate::types::events::EventKind;
        let sub = IpcMessage::subscribe(vec![EventKind::Window, EventKind::System]);
        let json = serde_json::to_string(&sub).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        match parsed.kind {
            MessageKind::Subscribe(s) => {
                assert_eq!(s.kinds.len(), 2);
            }
            _ => panic!("Expected Subscribe"),
        }
    }

    #[test]
    fn test_message_response_id_correlation() {
        let id = Uuid::new_v4();
        let resp = IpcMessage::response(id, Response::Ok { message: None });
        assert_eq!(resp.id, id);
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, id);
    }

    // ── Wire format compatibility tests ────────────────────────
    //
    // These verify that JSON produced by the bar's manual
    // construction matches what enad's types can deserialize.

    #[test]
    fn test_wire_format_unit_command() {
        // This is the format the bar's send_unit_command produces:
        // {"id": "...", "kind": {"type": "Command", "body": "CommandName"}}
        let wire = serde_json::json!({
            "id": Uuid::new_v4(),
            "kind": {
                "type": "Command",
                "body": "GetFirstRunStatus"
            }
        });
        let parsed: IpcMessage = serde_json::from_value(wire).unwrap();
        match parsed.kind {
            MessageKind::Command(cmd) => {
                assert!(matches!(cmd, Command::GetFirstRunStatus));
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_wire_format_struct_command() {
        // This is the format the bar's send_command produces:
        // {"id": "...", "kind": {"type": "Command", "body": {"CommandName": <body>}}}
        let wire = serde_json::json!({
            "id": Uuid::new_v4(),
            "kind": {
                "type": "Command",
                "body": {
                    "GetContextCommands": {
                        "query": "open browser",
                        "limit": 6
                    }
                }
            }
        });
        let parsed: IpcMessage = serde_json::from_value(wire).unwrap();
        match parsed.kind {
            MessageKind::Command(Command::GetContextCommands { query, limit }) => {
                assert_eq!(query, "open browser");
                assert_eq!(limit, Some(6));
            }
            _ => panic!("Expected GetContextCommands"),
        }
    }

    #[test]
    fn test_wire_format_ping() {
        let wire = serde_json::json!({
            "id": Uuid::new_v4(),
            "kind": {
                "type": "Ping",
                "body": null
            }
        });
        let parsed: IpcMessage = serde_json::from_value(wire).unwrap();
        assert!(matches!(parsed.kind, MessageKind::Ping));
    }

    #[test]
    fn test_wire_format_response() {
        // Enad sends Response with default serde (not adjacently tagged)
        let wire = serde_json::json!({
            "id": Uuid::new_v4(),
            "kind": {
                "type": "Response",
                "body": {
                    "Data": {
                        "payload": {
                            "commands": [],
                            "context": {}
                        }
                    }
                }
            }
        });
        let parsed: IpcMessage = serde_json::from_value(wire).unwrap();
        match parsed.kind {
            MessageKind::Response(Response::Data { payload }) => {
                assert!(payload.get("commands").is_some());
                assert!(payload.get("context").is_some());
            }
            _ => panic!("Expected Response::Data"),
        }
    }

    #[test]
    fn test_wire_format_response_error() {
        let wire = serde_json::json!({
            "id": Uuid::new_v4(),
            "kind": {
                "type": "Response",
                "body": {
                    "Error": {
                        "code": "NOT_FOUND",
                        "message": "Snapshot not found"
                    }
                }
            }
        });
        let parsed: IpcMessage = serde_json::from_value(wire).unwrap();
        match parsed.kind {
            MessageKind::Response(Response::Error { code, .. }) => {
                assert_eq!(code, "NOT_FOUND");
            }
            _ => panic!("Expected Response::Error"),
        }
    }

    #[test]
    fn test_wire_format_event() {
        let wire = serde_json::json!({
            "id": Uuid::new_v4(),
            "kind": {
                "type": "Event",
                "body": {
                    "id": Uuid::new_v4(),
                    "timestamp": "2025-01-15T10:30:00Z",
                    "source": "enad",
                    "kind": "Window",
                    "payload": {
                        "type": "WindowFocused",
                        "data": {
                            "app": "Alacritty",
                            "title": "~"
                        }
                    }
                }
            }
        });
        let parsed: IpcMessage = serde_json::from_value(wire).unwrap();
        match parsed.kind {
            MessageKind::Event(event) => {
                assert_eq!(event.source, "enad");
            }
            _ => panic!("Expected Event"),
        }
    }

    #[test]
    fn test_wire_format_subscribe() {
        let wire = serde_json::json!({
            "id": Uuid::new_v4(),
            "kind": {
                "type": "Subscribe",
                "body": {
                    "kinds": []
                }
            }
        });
        let parsed: IpcMessage = serde_json::from_value(wire).unwrap();
        match parsed.kind {
            MessageKind::Subscribe(sub) => {
                assert!(sub.kinds.is_empty());
            }
            _ => panic!("Expected Subscribe"),
        }
    }

    // ── Malformed / edge-case tests ────────────────────────────

    #[test]
    fn test_deserialize_missing_field_fails() {
        let result = serde_json::from_str::<IpcMessage>(r##"{"id": "..."}"##);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_wrong_type_tag_fails() {
        let result = serde_json::from_str::<IpcMessage>(
            r##"{"id": "00000000-0000-0000-0000-000000000000", "kind": {"type": "Unknown", "body": null}}"##,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_contains_kind_field() {
        let msg = IpcMessage::command(Command::GetContext);
        let json = serde_json::to_value(&msg).unwrap();
        assert!(
            json.get("kind").is_some(),
            "JSON must have top-level 'kind' field"
        );
        assert!(json.get("id").is_some(), "JSON must have 'id' field");
        // The inner kind must have "type" and "body" (adjacent tagging)
        let kind = json.get("kind").unwrap();
        assert!(
            kind.get("type").is_some(),
            "kind must have nested 'type' field"
        );
    }

    #[test]
    fn test_no_flatten_in_kind() {
        // Verify that serde(flatten) is NOT used on the kind field.
        let msg = IpcMessage::command(Command::GetContext);
        let json = serde_json::to_value(&msg).unwrap();
        // The top-level JSON should NOT have "type" or "body" directly.
        assert!(
            json.get("type").is_none(),
            "'type' must NOT be at top level (no flatten)"
        );
        assert!(
            json.get("body").is_none(),
            "'body' must NOT be at top level (no flatten)"
        );
        // Instead, "kind" should be a nested object containing "type" and "body".
        let kind = json.get("kind").unwrap();
        assert!(kind.is_object(), "'kind' must be a nested object");
        assert!(kind.get("type").is_some(), "'kind.type' must exist");
        assert!(kind.get("body").is_some(), "'kind.body' must exist");
    }
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
