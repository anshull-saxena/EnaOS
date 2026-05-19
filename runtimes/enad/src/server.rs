use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::actions::executor::ActionExecutor;
use crate::actions::types::{ActionRequest, ActionType};
use crate::bus::EventBus;
use crate::memory::store::MemoryStore;
use crate::memory::types::MemoryQuery;
use crate::orchestration::engine::OrchestrationEngine;
use crate::snapshot::capture;
use crate::snapshot::store::SnapshotStore;
use crate::restore::plan::RestorePlanner;
use crate::restore::types::{RestoreSelections, RestoreResult};
use crate::suggestion::engine::SuggestionEngine;
use crate::context::ContextEngine;
use crate::types::ipc::{Command, IpcMessage, MessageKind, Response, StateTarget};

/// IPC server that listens on a Unix domain socket.
pub struct IpcServer {
    listener: UnixListener,
    bus: Arc<EventBus>,
    action_executor: Arc<ActionExecutor>,
    memory_store: Arc<MemoryStore>,
    snapshot_store: Arc<SnapshotStore>,
    orchestration: Arc<OrchestrationEngine>,
    suggestion_engine: Arc<SuggestionEngine>,
    context_engine: Arc<ContextEngine>,
    /// Shutdown signal sender.
    shutdown_tx: broadcast::Sender<()>,
}

impl IpcServer {
    pub fn bind(
        path: &str,
        bus: Arc<EventBus>,
        action_executor: Arc<ActionExecutor>,
        memory_store: Arc<MemoryStore>,
        snapshot_store: Arc<SnapshotStore>,
        orchestration: Arc<OrchestrationEngine>,
        suggestion_engine: Arc<SuggestionEngine>,
        context_engine: Arc<ContextEngine>,
    ) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(path);

        let listener = UnixListener::bind(path)?;
        let (shutdown_tx, _) = broadcast::channel(1);

        info!("IPC server listening on {}", path);

        Ok(Self {
            listener,
            bus,
            action_executor,
            memory_store,
            snapshot_store,
            orchestration,
            suggestion_engine,
            context_engine,
            shutdown_tx,
        })
    }

    /// Run the accept loop — spawns one task per connection.
    pub async fn run(&self) {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let bus = self.bus.clone();
                            let executor = self.action_executor.clone();
                            let memory = self.memory_store.clone();
                            let snapshots = self.snapshot_store.clone();
                            let orch = self.orchestration.clone();
                            let suggestion = self.suggestion_engine.clone();
                            let context = self.context_engine.clone();
                            let shutdown_rx = self.shutdown_tx.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, bus, executor, memory, snapshots, orch, suggestion, context, shutdown_rx).await {
                                    warn!("Connection handler error: {e}");
                                }
                            });
                            let _ = addr;
                        }
                        Err(e) => {
                            error!("Accept error: {e}");
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("IPC server shutting down");
                    break;
                }
            }
        }
    }

    /// Get a shutdown sender.
    pub fn shutdown_handle(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }
}

/// Handle a single client connection over the Unix domain socket.
async fn handle_connection(
    mut stream: UnixStream,
    bus: Arc<EventBus>,
    executor: Arc<ActionExecutor>,
    memory: Arc<MemoryStore>,
    snapshots: Arc<SnapshotStore>,
    orchestration: Arc<OrchestrationEngine>,
    suggestion: Arc<SuggestionEngine>,
    context: Arc<ContextEngine>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let mut event_rx = bus.subscribe_all();

    loop {
        tokio::select! {
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => {
                        info!("Client disconnected");
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            line.clear();
                            continue;
                        }

                        match serde_json::from_str::<IpcMessage>(trimmed) {
                            Ok(msg) => {
                                let response = dispatch(msg, &bus, &executor, &memory, &snapshots, &orchestration, &suggestion, &context).await;
                                let response_json = serde_json::to_string(&response)?;
                                writer.write_all(response_json.as_bytes()).await?;
                                writer.write_all(b"\n").await?;
                                writer.flush().await?;
                            }
                            Err(e) => {
                                let err = IpcMessage::response(
                                    uuid::Uuid::nil(),
                                    Response::Error {
                                        code: "PARSE_ERROR".into(),
                                        message: format!("Invalid JSON: {e}"),
                                    },
                                );
                                let json = serde_json::to_string(&err)?;
                                writer.write_all(json.as_bytes()).await?;
                                writer.write_all(b"\n").await?;
                                writer.flush().await?;
                            }
                        }
                        line.clear();
                    }
                    Err(e) => {
                        warn!("Read error: {e}");
                        break;
                    }
                }
            }
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        let msg = IpcMessage::event(event);
                        let json = serde_json::to_string(&msg)?;
                        if let Err(e) = writer.write_all(json.as_bytes()).await {
                            warn!("Failed to push event to client: {e}");
                            break;
                        }
                        let _ = writer.write_all(b"\n").await;
                        let _ = writer.flush().await;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Event bus lagged by {n} messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }

    Ok(())
}

/// Dispatch an IPC message and produce a response.
async fn dispatch(
    msg: IpcMessage,
    bus: &EventBus,
    executor: &ActionExecutor,
    memory: &MemoryStore,
    snapshots: &SnapshotStore,
    orchestration: &OrchestrationEngine,
    suggestion: &SuggestionEngine,
    context: &ContextEngine,
) -> IpcMessage {
    let id = msg.id;

    match msg.kind {
        MessageKind::Command(cmd) => {
            let response = handle_command(cmd, bus, executor, memory, snapshots, orchestration, suggestion, context).await;
            IpcMessage::response(id, response)
        }
        MessageKind::Subscribe(sub) => {
            IpcMessage::response(id, Response::Ok {
                message: Some(format!("Subscribed to {} event kind(s)", sub.kinds.len())),
            })
        }
        MessageKind::Ping => {
            let latency = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            IpcMessage::response(id, Response::Data {
                payload: serde_json::json!({
                    "code": "PONG",
                    "latency_ms": latency,
                }),
            })
        }
        _ => IpcMessage::response(id, Response::Error {
            code: "UNEXPECTED".into(),
            message: "Unexpected message kind from client".into(),
        }),
    }
}

/// Handle an IPC command.
async fn handle_command(
    cmd: Command,
    bus: &EventBus,
    executor: &ActionExecutor,
    memory: &MemoryStore,
    snapshots: &SnapshotStore,
    orchestration: &OrchestrationEngine,
    suggestion: &SuggestionEngine,
    context: &ContextEngine,
) -> Response {
    match cmd {
        Command::Execute { command, args } => {
            Response::Ok {
                message: Some(format!("Executing: {} {:?}", command, args)),
            }
        }
        Command::ExecuteAction { action, params } => {
            // Parse the action type from the string + params.
            let action_type = parse_action_type(&action, &params);

            match action_type {
                Ok(action_type) => {
                    let permission = ActionRequest::default_permission(&action_type);
                    let request = ActionRequest::new(action_type, permission);

                    match executor.execute(request).await {
                        Ok(action_id) => Response::Data {
                            payload: serde_json::json!({
                                "action_id": action_id,
                                "status": "started",
                            }),
                        },
                        Err(error) => Response::Error {
                            code: "ACTION_FAILED".into(),
                            message: error,
                        },
                    }
                }
                Err(error) => Response::Error {
                    code: "INVALID_ACTION".into(),
                    message: error,
                },
            }
        }
        Command::CancelAction { action_id } => {
            match executor.cancel(action_id).await {
                Ok(()) => Response::Ok {
                    message: Some(format!("Action {action_id} cancelled")),
                },
                Err(error) => Response::Error {
                    code: "CANCEL_FAILED".into(),
                    message: error,
                },
            }
        }
        Command::SpawnAgent { task, capabilities } => {
            let agent_id = uuid::Uuid::new_v4();

            bus.publish(crate::types::events::SystemEvent::new(
                "enad",
                crate::types::events::EventKind::Agent,
                crate::types::events::EventPayload::AgentSpawned {
                    agent_id,
                    task: task.clone(),
                },
            ));

            Response::Data {
                payload: serde_json::json!({
                    "agent_id": agent_id,
                    "task": task,
                    "capabilities": capabilities,
                    "status": "spawned",
                }),
            }
        }
        Command::QueryState { target } => match target {
            StateTarget::SystemInfo => Response::Data {
                payload: serde_json::json!({
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                    "hostname": hostname(),
                    "uptime": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                }),
            },
            StateTarget::ActiveWindows => Response::Data {
                payload: serde_json::json!({
                    "windows": [],
                    "message": "Window tracking active via system module",
                }),
            },
            StateTarget::RunningAgents => Response::Data {
                payload: serde_json::json!({
                    "agents": [],
                }),
            },
            StateTarget::ProcessList => Response::Data {
                payload: serde_json::json!({
                    "processes": [],
                }),
            },
            StateTarget::DesktopContext => Response::Data {
                payload: serde_json::json!({
                    "desktop_integration": true,
                    "subsystems": [
                        "upower",
                        "networkmanager",
                        "window-tracker",
                        "workspace",
                        "clipboard",
                        "notifications",
                        "audio",
                        "mpris"
                    ],
                }),
            },
            StateTarget::MemoryRecent => {
                let mut q = MemoryQuery::new();
                q.limit = 20;
                match memory.query(&q) {
                    Ok(entries) => Response::Data {
                        payload: serde_json::to_value(&entries).unwrap_or(serde_json::json!([])),
                    },
                    Err(e) => Response::Error {
                        code: "MEMORY_QUERY".into(),
                        message: e,
                    },
                }
            }
            StateTarget::MemorySummary => {
                match memory.summary() {
                    Ok(summary) => Response::Data {
                        payload: serde_json::to_value(&summary).unwrap_or(serde_json::json!({})),
                    },
                    Err(e) => Response::Error {
                        code: "MEMORY_SUMMARY".into(),
                        message: e,
                    },
                }
            }
            StateTarget::MemorySearch { query } => {
                let q = MemoryQuery::search(&query);
                match memory.query(&q) {
                    Ok(entries) => Response::Data {
                        payload: serde_json::to_value(&entries).unwrap_or(serde_json::json!([])),
                    },
                    Err(e) => Response::Error {
                        code: "MEMORY_SEARCH".into(),
                        message: e,
                    },
                }
            }
        },
        Command::Terminate { id } => Response::Ok {
            message: Some(format!("Terminate request for {id} (not yet implemented)")),
        },
        Command::GetContext => Response::Data {
            payload: serde_json::json!({
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "hostname": hostname(),
                "desktop": "EnaOS",
                "version": env!("CARGO_PKG_VERSION"),
            }),
        },
        // ── Orchestration commands ──
        Command::SubmitPlan { plan } => {
            match serde_json::from_value::<crate::orchestration::types::ExecutionPlan>(plan) {
                Ok(plan) => {
                    let plan_id = orchestration.submit_plan(plan).await;
                    Response::Data {
                        payload: serde_json::json!({
                            "plan_id": plan_id,
                            "status": "submitted",
                        }),
                    }
                }
                Err(e) => Response::Error {
                    code: "INVALID_PLAN".into(),
                    message: format!("Invalid plan: {e}"),
                },
            }
        }
        Command::ApprovePlan { plan_id } => {
            match orchestration.approve_plan(plan_id).await {
                Ok(()) => Response::Ok {
                    message: Some(format!("Plan {plan_id} approved")),
                },
                Err(e) => Response::Error {
                    code: "APPROVE_FAILED".into(),
                    message: e,
                },
            }
        }
        Command::RejectPlan { plan_id } => {
            match orchestration.reject_plan(plan_id).await {
                Ok(()) => Response::Ok {
                    message: Some(format!("Plan {plan_id} rejected")),
                },
                Err(e) => Response::Error {
                    code: "REJECT_FAILED".into(),
                    message: e,
                },
            }
        }
        Command::CancelPlan { plan_id } => {
            match orchestration.cancel_plan(plan_id).await {
                Ok(()) => Response::Ok {
                    message: Some(format!("Plan {plan_id} cancelled")),
                },
                Err(e) => Response::Error {
                    code: "CANCEL_FAILED".into(),
                    message: e,
                },
            }
        }
        Command::ListPlans => {
            let plans = orchestration.list_plans().await;
            Response::Data {
                payload: serde_json::to_value(&plans).unwrap_or(serde_json::json!([])),
            }
        }
        // ── Workspace Snapshot commands ──
        Command::TakeSnapshot { label } => {
            let label = label.unwrap_or_else(|| "Manual snapshot".to_string());
            let snapshot = capture::take_immediate_snapshot(
                snapshots, memory, orchestration, &label, bus,
            ).await;
            match snapshot {
                Ok(snapshot_id) => {
                    bus.publish(crate::types::events::SystemEvent::new(
                        "enad",
                        crate::types::events::EventKind::System,
                        crate::types::events::EventPayload::SnapshotTaken {
                            snapshot_id,
                            label,
                            node_count: 0,
                        },
                    ));
                    Response::Data {
                        payload: serde_json::json!({
                            "snapshot_id": snapshot_id,
                            "status": "taken",
                        }),
                    }
                }
                Err(e) => Response::Error {
                    code: "SNAPSHOT_FAILED".into(),
                    message: e,
                },
            }
        }
        Command::ListSnapshots { limit } => {
            let limit = limit.unwrap_or(20) as usize;
            match snapshots.list(limit) {
                Ok(summaries) => Response::Data {
                    payload: serde_json::to_value(&summaries).unwrap_or(serde_json::json!([])),
                },
                Err(e) => Response::Error {
                    code: "SNAPSHOT_LIST".into(),
                    message: e,
                },
            }
        }
        Command::GetSnapshot { snapshot_id } => {
            match snapshots.get(&snapshot_id) {
                Ok(Some(snapshot)) => Response::Data {
                    payload: serde_json::to_value(&snapshot).unwrap_or(serde_json::json!({})),
                },
                Ok(None) => Response::Error {
                    code: "NOT_FOUND".into(),
                    message: format!("Snapshot {snapshot_id} not found"),
                },
                Err(e) => Response::Error {
                    code: "SNAPSHOT_GET".into(),
                    message: e,
                },
            }
        }
        Command::DeleteSnapshot { snapshot_id } => {
            match snapshots.delete(&snapshot_id) {
                Ok(true) => {
                    bus.publish(crate::types::events::SystemEvent::new(
                        "enad",
                        crate::types::events::EventKind::System,
                        crate::types::events::EventPayload::SnapshotDeleted { snapshot_id },
                    ));
                    Response::Ok {
                        message: Some(format!("Snapshot {snapshot_id} deleted")),
                    }
                }
                Ok(false) => Response::Error {
                    code: "NOT_FOUND".into(),
                    message: format!("Snapshot {snapshot_id} not found"),
                },
                Err(e) => Response::Error {
                    code: "SNAPSHOT_DELETE".into(),
                    message: e,
                },
            }
        }
        // ── Restoration commands ──
        Command::PreviewRestore { snapshot_id } => {
            let planner = RestorePlanner;
            match snapshots.get(&snapshot_id) {
                Ok(Some(snapshot)) => {
                    let preview = planner.preview(&snapshot);
                    Response::Data {
                        payload: serde_json::to_value(&preview).unwrap_or(serde_json::json!({})),
                    }
                }
                Ok(None) => Response::Error {
                    code: "NOT_FOUND".into(),
                    message: format!("Snapshot {snapshot_id} not found"),
                },
                Err(e) => Response::Error {
                    code: "SNAPSHOT_GET".into(),
                    message: e,
                },
            }
        }
        Command::RestoreSnapshot { snapshot_id, selections } => {
            let planner = RestorePlanner;

            let snapshot = match snapshots.get(&snapshot_id) {
                Ok(Some(s)) => s,
                Ok(None) => return Response::Error {
                    code: "NOT_FOUND".into(),
                    message: format!("Snapshot {snapshot_id} not found"),
                },
                Err(e) => return Response::Error {
                    code: "SNAPSHOT_GET".into(),
                    message: e,
                },
            };

            // Parse optional selection filters.
            let selections = match selections {
                Some(val) => serde_json::from_value::<RestoreSelections>(val).ok(),
                None => None,
            };

            // Build the restoration plan.
            let plan = planner.plan_restoration(&snapshot, selections.as_ref());
            let plan_id = plan.id;
            let action_count = plan.nodes.len() as u32;

            // Submit to orchestration engine.
            orchestration.submit_plan(plan).await;

            // Mark snapshot as restored.
            let _ = snapshots.mark_restored(&snapshot_id);

            // Emit event.
            bus.publish(crate::types::events::SystemEvent::new(
                "enad",
                crate::types::events::EventKind::System,
                crate::types::events::EventPayload::RestoreStarted {
                    snapshot_id,
                    plan_id,
                    description: format!("Restoration of {} with {action_count} actions", snapshot.label),
                },
            ));

            Response::Data {
                payload: serde_json::to_value(&RestoreResult {
                    snapshot_id,
                    plan_id,
                    action_count,
                }).unwrap_or(serde_json::json!({})),
            }
        }

        // ── Ambient suggestion commands ──
        Command::GetSuggestions { limit } => {
            let limit = limit.unwrap_or(5) as usize;
            suggestion.get_suggestions(limit)
        }
        Command::DismissSuggestion { suggestion_id, permanent } => {
            suggestion.dismiss_suggestion(&suggestion_id, permanent.unwrap_or(false))
        }

        // ── Contextual Command Intelligence ──
        Command::GetContextCommands { query, limit } => {
            let limit = limit.unwrap_or(6) as usize;
            let suggestions = context.resolve(&query);
            let suggestions: Vec<_> = suggestions.into_iter().take(limit).collect();
            Response::Data {
                payload: serde_json::json!({
                    "commands": suggestions,
                    "context": context.context_snapshot(),
                }),
            }
        }
    }
}

/// Parse an action type from a string name and params JSON.
fn parse_action_type(name: &str, params: &serde_json::Value) -> Result<ActionType, String> {
    match name {
        "open_app" => Ok(ActionType::OpenApp {
            app: params.get("app").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        "open_url" => Ok(ActionType::OpenUrl {
            url: params.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        "focus_window" => Ok(ActionType::FocusWindow {
            app: params.get("app").and_then(|v| v.as_str()).map(|s| s.to_string()),
            title: params.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
        }),
        "launch_command" => Ok(ActionType::LaunchCommand {
            command: params.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            args: params.get("args")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
        }),
        "switch_workspace" => Ok(ActionType::SwitchWorkspace {
            workspace: params.get("workspace").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        "search_files" => Ok(ActionType::SearchFiles {
            query: params.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            path: params.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()),
        }),
        "media_control" => Ok(ActionType::MediaControl {
            action: params.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        "clipboard_set" => Ok(ActionType::ClipboardSet {
            text: params.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        "read_window_title" => Ok(ActionType::ReadWindowTitle),
        "notify" => Ok(ActionType::Notify {
            title: params.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            body: params.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        _ => Err(format!("Unknown action: {name}")),
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .or_else(|_| {
            std::process::Command::new("hostname")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .map_err(|e: std::io::Error| e)
        })
        .unwrap_or_else(|_| "enaos".into())
}
