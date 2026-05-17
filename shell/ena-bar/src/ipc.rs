use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};
use uuid::Uuid;

// ── IPC Protocol (matches enad's types/ipc.rs) ───────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IpcMessage {
    id: Uuid,
    #[serde(flatten)]
    kind: MessageKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "body")]
enum MessageKind {
    Command(Command),
    Response(Response),
    Subscribe(Subscription),
    Event(SystemEvent),
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Command {
    Execute { command: String, args: Vec<String> },
    SpawnAgent { task: String, capabilities: Vec<String> },
    QueryState { target: String },
    Terminate { id: Uuid },
    GetContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Response {
    Ok { message: Option<String> },
    Data { payload: Value },
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Subscription {
    kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemEvent {
    id: Uuid,
    timestamp: String,
    source: String,
    kind: String,
    payload: EventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventPayload {
    #[serde(flatten)]
    data: Value,
}

// ── EnaBar Events ────────────────────────────────────────────────

/// Represents an IPC event from enad.
#[derive(Debug, Clone)]
pub enum EnadEvent {
    /// Ping response with latency info.
    Pong { latency_ms: u64 },
    /// System event forwarded from enad's event bus.
    SystemEvent { kind: String, payload: Value },
    /// Connection established.
    Connected,
    /// Connection lost.
    Disconnected,
    /// Raw JSON we couldn't parse.
    Raw(String),
}

/// Run the IPC client in a background thread.
///
/// Connects to enad's Unix socket, subscribes to all events,
/// and forwards them to the GTK main loop via the mpsc sender.
pub fn run(
    socket_path: String,
    running: Arc<AtomicBool>,
    sender: mpsc::Sender<EnadEvent>,
) {
    info!("IPC client starting, connecting to {socket_path}");

    loop {
        if !running.load(Ordering::Relaxed) {
            info!("IPC client shutting down");
            return;
        }

        match connect_and_listen(&socket_path, &sender, &running) {
            Ok(()) => {
                warn!("IPC connection closed by server, reconnecting in 2s...");
            }
            Err(e) => {
                error!("IPC connection error: {e}, reconnecting in 2s...");
            }
        }

        let _ = sender.send(EnadEvent::Disconnected);
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

fn connect_and_listen(
    socket_path: &str,
    sender: &mpsc::Sender<EnadEvent>,
    running: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
    info!("Connected to enad at {socket_path}");

    sender.send(EnadEvent::Connected).ok();

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    // Subscribe to all events using the correct IpcMessage format.
    let subscribe = IpcMessage {
        id: Uuid::new_v4(),
        kind: MessageKind::Subscribe(Subscription { kinds: vec![] }),
    };
    let json = serde_json::to_string(&subscribe)?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;

    // Send initial ping.
    send_ping(&mut writer);

    let mut line = String::new();

    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                info!("enad closed the connection");
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(json) => {
                        let event = parse_event(json);
                        let _ = sender.send(event);
                    }
                    Err(e) => {
                        warn!("Failed to parse IPC message: {e}, raw: {trimmed}");
                        let _ = sender.send(EnadEvent::Raw(trimmed.to_string()));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Read timeout — send keepalive ping
                send_ping(&mut writer);
                continue;
            }
            Err(e) => {
                error!("Read error: {e}");
                break;
            }
        }
    }

    Ok(())
}

fn send_ping(writer: &mut UnixStream) {
    let ping = IpcMessage {
        id: Uuid::new_v4(),
        kind: MessageKind::Ping,
    };
    if let Ok(json) = serde_json::to_string(&ping) {
        if writeln!(writer, "{}", json).is_err() {
            error!("Failed to send keepalive ping");
        }
        let _ = writer.flush();
    }
}

fn parse_event(json: Value) -> EnadEvent {
    // Try to parse as IpcMessage first.
    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
        match msg_type {
            "Pong" => {
                return EnadEvent::Pong { latency_ms: 0 };
            }
            "Event" => {
                // Extract the SystemEvent from the body.
                if let Some(body) = json.get("body") {
                    // The event kind (Window, System, Audio, etc.)
                    let kind = body
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    // The payload contains { "type": "...", "data": {...} }
                    // We want to pass the full payload so the bar can extract type + data.
                    let payload = body.get("payload").cloned().unwrap_or(Value::Null);

                    return EnadEvent::SystemEvent { kind, payload };
                }
            }
            "Response" => {
                // Check if it's a response to a ping (PONG).
                if let Some(body) = json.get("body") {
                    if let Some(code) = body.get("code").and_then(|v| v.as_str()) {
                        if code == "PONG" {
                            let latency = body
                                .get("latency_ms")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            return EnadEvent::Pong { latency_ms: latency };
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Fallback: try the old flat format for backwards compatibility.
    match json.get("kind").and_then(|k| k.as_str()) {
        Some("pong") => {
            let latency = json
                .get("payload")
                .and_then(|p| p.get("latency_ms"))
                .and_then(|l| l.as_u64())
                .unwrap_or(0);
            EnadEvent::Pong { latency_ms: latency }
        }
        Some(kind) => {
            let payload = json.get("payload").cloned().unwrap_or(Value::Null);
            EnadEvent::SystemEvent {
                kind: kind.to_string(),
                payload,
            }
        }
        None => {
            // Try to extract from nested structure.
            let kind = json
                .get("payload")
                .and_then(|p| p.get("kind"))
                .and_then(|k| k.as_str())
                .unwrap_or("unknown")
                .to_string();
            let payload = json.get("payload").cloned().unwrap_or(json);
            EnadEvent::SystemEvent { kind, payload }
        }
    }
}
