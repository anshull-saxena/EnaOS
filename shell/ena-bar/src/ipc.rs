use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::command_palette::CommandSuggestion;

// ── IPC Protocol (matches enad's types/ipc.rs) ───────────────────
//
// Envelope format:
//   Bar → Daemon:  {"id": "...", "kind": {"type": "Command", "body": <command>}}
//   Daemon → Bar:  {"id": "...", "kind": {"type": "Event" | "Response", "body": <body>}}
//
// The `kind` field is a nested object (NOT flattened) so that enad's
// IpcMessage deserialization (which expects `kind` as a field) works.

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IpcMessage {
    id: Uuid,
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
    Execute {
        command: String,
        args: Vec<String>,
    },
    SpawnAgent {
        task: String,
        capabilities: Vec<String>,
    },
    QueryState {
        target: String,
    },
    Terminate {
        id: Uuid,
    },
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
pub fn run(socket_path: String, running: Arc<AtomicBool>, sender: mpsc::Sender<EnadEvent>) {
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

/// Send a unit-variant command to enad (e.g. GetFirstRunStatus, CompleteOnboarding).
///
/// Opens a fresh Unix socket connection, sends the command with the
/// correct `{"kind": {"type": "Command", "body": "CommandName"}}` envelope.
pub fn send_unit_command(socket_path: &str, command: &str) -> Result<Value, String> {
    let stream = UnixStream::connect(socket_path).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| format!("set_read_timeout: {e}"))?;

    let mut writer = stream.try_clone().map_err(|e| format!("clone: {e}"))?;
    let mut reader = BufReader::new(stream);

    // Unit variants serialize as just the command name string in the body.
    // Full wire format: {"id": "...", "kind": {"type": "Command", "body": "CommandName"}}
    let msg = json!({
        "id": Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": command
        }
    });

    let json = serde_json::to_string(&msg).map_err(|e| format!("serialize: {e}"))?;
    writeln!(writer, "{json}").map_err(|e| format!("write: {e}"))?;
    writer.flush().map_err(|e| format!("flush: {e}"))?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read: {e}"))?;

    serde_json::from_str(&line).map_err(|e| format!("parse: {e}"))
}

/// Send a struct-variant command to enad with an object body.
///
/// Wire format:
///   {"id": "...", "kind": {"type": "Command", "body": {"CommandName": <body>}}}
pub fn send_command(socket_path: &str, command: &str, body: &Value) -> Result<Value, String> {
    let stream = UnixStream::connect(socket_path).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| format!("set_read_timeout: {e}"))?;

    let mut writer = stream.try_clone().map_err(|e| format!("clone: {e}"))?;
    let mut reader = BufReader::new(stream);

    // Proper envelope: command wrapped in kind field with tag+content.
    let msg = json!({
        "id": Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": {
                command: body
            }
        }
    });

    let json = serde_json::to_string(&msg).map_err(|e| format!("serialize: {e}"))?;
    writeln!(writer, "{json}").map_err(|e| format!("write: {e}"))?;
    writer.flush().map_err(|e| format!("flush: {e}"))?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read: {e}"))?;

    serde_json::from_str(&line).map_err(|e| format!("parse: {e}"))
}

/// Fetch context-aware command suggestions from enad.
///
/// Opens a fresh Unix socket, sends GetContextCommands using the correct
/// wire envelope, parses the response into a Vec<CommandSuggestion>.
///
/// Response wire format:
///   {"id": "...", "kind": {"type": "Response", "body": {"Data": {"payload": {"commands": [...], "context": {...}}}}}}
pub fn get_context_commands(
    socket_path: &str,
    query: &str,
    limit: u32,
) -> Result<Vec<CommandSuggestion>, String> {
    let stream = UnixStream::connect(socket_path).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| format!("set_read_timeout: {e}"))?;

    let mut writer = stream.try_clone().map_err(|e| format!("clone: {e}"))?;
    let mut reader = BufReader::new(stream);

    // Correct envelope: kind → { type, body } → { GetContextCommands: {...} }
    let msg = json!({
        "id": Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": {
                "GetContextCommands": {
                    "query": query,
                    "limit": limit
                }
            }
        }
    });

    let json = serde_json::to_string(&msg).map_err(|e| format!("serialize: {e}"))?;
    writeln!(writer, "{json}").map_err(|e| format!("write: {e}"))?;
    writer.flush().map_err(|e| format!("flush: {e}"))?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read: {e}"))?;

    let response: Value = serde_json::from_str(&line).map_err(|e| format!("parse: {e}"))?;

    // Navigate: kind → body → Data → payload → commands
    if let Some(kind) = response.get("kind")
        && let Some(body) = kind.get("body")
    {
        // body is { "Data": { "payload": { "commands": [...], "context": {...} } } }
        if let Some(data_body) = body.get("Data")
            && let Some(payload) = data_body.get("payload")
            && let Some(commands) = payload.get("commands").and_then(|c| c.as_array())
        {
            let parsed: Result<Vec<CommandSuggestion>, _> = commands
                .iter()
                .map(|s| serde_json::from_value(s.clone()))
                .collect();
            return parsed.map_err(|e| format!("parse_suggestions: {e}"));
        }
    }

    Ok(Vec::new())
}

/// Parse an incoming IPC message (JSON Value) into an EnadEvent.
///
/// Enad's wire format:
///   {"id": "...", "kind": {"type": "Event | Response | Pong", "body": ...}}
///
/// We navigate via the top-level "kind" field, not flattened fields.
fn parse_event(json: Value) -> EnadEvent {
    // Check the "type" discriminator under "kind".
    if let Some(kind_val) = json.get("kind")
        && let Some(msg_type) = kind_val.get("type").and_then(|v| v.as_str())
    {
        match msg_type {
            "Pong" => {
                return EnadEvent::Pong { latency_ms: 0 };
            }
            "Event" => {
                // Extract the SystemEvent from kind.body.
                if let Some(body) = kind_val.get("body") {
                    // The event kind (Window, System, Audio, etc.)
                    let event_kind = body
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    // The payload is the body's "payload" field.
                    let payload = body.get("payload").cloned().unwrap_or(Value::Null);

                    return EnadEvent::SystemEvent {
                        kind: event_kind,
                        payload,
                    };
                }
            }
            "Response" => {
                // Check if it's a response to a ping (PONG).
                if let Some(body) = kind_val.get("body") {
                    // body is { "Ok": { ... } } or { "Data": { ... } } or { "Error": { ... } }
                    // Pong responses have payload.code == "PONG" inside the Data variant.
                    if let Some(data) = body.get("Data")
                        && let Some(payload) = data.get("payload")
                        && let Some(code) = payload.get("code").and_then(|v| v.as_str())
                        && code == "PONG"
                    {
                        let latency = payload
                            .get("latency_ms")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        return EnadEvent::Pong {
                            latency_ms: latency,
                        };
                    }
                }
                // Non-pong responses are returned as Raw — the bar doesn't
                // route them through the event stream.
            }
            _ => {}
        }
    }

    // Fallback: parse_event is only called for messages on the persistent
    // event subscription socket. Unrecognised messages go to Raw.
    EnadEvent::Raw(format!("{}", json))
}
