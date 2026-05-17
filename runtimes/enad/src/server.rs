use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::actions::executor::ActionExecutor;
use crate::actions::types::{ActionRequest, ActionType};
use crate::bus::EventBus;
use crate::memory::store::MemoryStore;
use crate::memory::types::MemoryQuery;
use crate::types::ipc::{Command, IpcMessage, MessageKind, Response, StateTarget};

/// IPC server that listens on a Unix domain socket.
pub struct IpcServer {
    listener: UnixListener,
    bus: Arc<EventBus>,
    action_executor: Arc<ActionExecutor>,
    memory_store: Arc<MemoryStore>,
    /// Shutdown signal sender.
    shutdown_tx: broadcast::Sender<()>,
}

impl IpcServer {
    /// Bind to a Unix domain socket path.
    pub fn bind(path: &str, bus: Arc<EventBus>, action_executor: Arc<ActionExecutor>, memory_store: Arc<MemoryStore>) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(path);

        let listener = UnixListener::bind(path)?;
        let (shutdown_tx, _) = broadcast::channel(1);

        info!("IPC server listening on {}", path);

        Ok(Self {
            listener,
            bus,
            action_executor,
            memory_store,
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
                            let shutdown_rx = self.shutdown_tx.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, bus, executor, memory, shutdown_rx).await {
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
                                let response = dispatch(msg, &bus, &executor, &memory).await;
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
async fn dispatch(msg: IpcMessage, bus: &EventBus, executor: &ActionExecutor, memory: &MemoryStore) -> IpcMessage {
    let id = msg.id;

    match msg.kind {
        MessageKind::Command(cmd) => {
            let response = handle_command(cmd, bus, executor, memory).await;
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
async fn handle_command(cmd: Command, bus: &EventBus, executor: &ActionExecutor, memory: &MemoryStore) -> Response {
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
