use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::bus::EventBus;
use crate::types::ipc::{Command, IpcMessage, MessageKind, Response, StateTarget};

/// IPC server that listens on a Unix domain socket.
pub struct IpcServer {
    listener: UnixListener,
    bus: Arc<EventBus>,
    /// Shutdown signal sender.
    shutdown_tx: broadcast::Sender<()>,
}

impl IpcServer {
    /// Bind to a Unix domain socket path.
    pub fn bind(path: &str, bus: Arc<EventBus>) -> std::io::Result<Self> {
        // Remove stale socket if present.
        let _ = std::fs::remove_file(path);

        let listener = UnixListener::bind(path)?;
        let (shutdown_tx, _) = broadcast::channel(1);

        info!("IPC server listening on {}", path);

        Ok(Self {
            listener,
            bus,
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
                            let shutdown_rx = self.shutdown_tx.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, bus, shutdown_rx).await {
                                    warn!("Connection handler error: {e}");
                                }
                            });
                            // addr is () for Unix sockets, ignore
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
    mut shutdown_rx: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Subscribe to all events to push them to the client.
    let mut event_rx = bus.subscribe_all();

    loop {
        tokio::select! {
            // Read a JSON line from the client.
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => {
                        // EOF — client disconnected.
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
                                let response = dispatch(msg, &bus).await;
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
            // Push events from the bus to the client.
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
async fn dispatch(msg: IpcMessage, bus: &EventBus) -> IpcMessage {
    let id = msg.id;

    match msg.kind {
        MessageKind::Command(cmd) => {
            let response = handle_command(cmd, bus).await;
            IpcMessage::response(id, response)
        }
        MessageKind::Subscribe(sub) => {
            // Subscriptions are per-connection in handle_connection.
            // Acknowledge receipt.
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
async fn handle_command(cmd: Command, bus: &EventBus) -> Response {
    match cmd {
        Command::Execute { command, args } => {
            // Stub — will integrate with process manager.
            Response::Ok {
                message: Some(format!("Executing: {} {:?}", command, args)),
            }
        }
        Command::SpawnAgent { task, capabilities } => {
            let agent_id = uuid::Uuid::new_v4();

            // Publish agent spawned event.
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
            StateTarget::ProcessList => {
                // Query the process manager for tracked processes.
                // We need access to ProcessManager here — for now return stub.
                Response::Data {
                    payload: serde_json::json!({
                        "processes": [],
                    }),
                }
            }
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

fn hostname() -> String {
    // Try Linux path first, then macOS-compatible fallback.
    std::fs::read_to_string("/etc/hostname")
        .or_else(|_| {
            std::process::Command::new("hostname")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .map_err(|e: std::io::Error| e)
        })
        .unwrap_or_else(|_| "enaos".into())
}
