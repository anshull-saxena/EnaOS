/// Memory event capture — subscribes to enad event bus and stores relevant events.
///
/// Captures:
///   - Window focus changes → Event memory
///   - Workspace changes → Event + WorkspaceSnapshot
///   - Action executions → Action memory
///   - System state changes → ContextSnapshot (periodic)
///   - Network/battery changes → Event memory
///   - Clipboard updates → Event memory (preview only)
///   - Media playback changes → Event memory

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::bus::EventBus;
use crate::memory::store::MemoryStore;
use crate::memory::types::MemoryType;
use crate::types::events::{EventPayload, SystemEvent};

/// Memory event capturer.
pub struct MemoryCapture {
    store: Arc<MemoryStore>,
}

impl MemoryCapture {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }

    /// Start capturing events from the event bus.
    pub async fn run(&self, bus: Arc<EventBus>) {
        info!("Memory capture: starting event subscription");

        let mut rx = bus.subscribe_all();

        // Periodic context snapshot (every 60 seconds).
        let store_snapshot = self.store.clone();
        let snapshot_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                // Snapshot is triggered by external context updates, not here.
                // This task just ensures periodic cleanup.
                let _ = store_snapshot.expire(24);
            }
        });

        loop {
            match rx.recv().await {
                Ok(event) => {
                    self.capture_event(&event);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Memory capture: lagged by {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Memory capture: event bus closed");
                    break;
                }
            }
        }

        snapshot_task.abort();
    }

    /// Capture a single event into memory.
    fn capture_event(&self, event: &SystemEvent) {
        let workspace = None; // Workspace is tracked separately.

        match &event.payload {
            // Window events → memory.
            EventPayload::WindowFocused { app, title } => {
                let summary = format!("Focused: {app} — {title}");
                let details = serde_json::json!({
                    "app": app,
                    "title": title,
                    "source": &event.source,
                });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            EventPayload::WindowOpened { app, pid } => {
                let summary = format!("Opened: {app} (pid {pid})");
                let details = serde_json::json!({ "app": app, "pid": pid });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            EventPayload::WindowClosed { app, pid } => {
                let summary = format!("Closed: {app} (pid {pid})");
                let details = serde_json::json!({ "app": app, "pid": pid });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            // Workspace events → memory + snapshot.
            EventPayload::WorkspaceChanged { workspace: ws, output } => {
                let summary = format!("Switched to: {ws}");
                let details = serde_json::json!({
                    "workspace": ws,
                    "output": output,
                });
                let _ = self.store.insert(MemoryType::WorkspaceSnapshot, Some(ws), &summary, &details);
            }

            // Battery/network → memory.
            EventPayload::BatteryStatus { percentage, state, time_to_empty, time_to_full } => {
                let summary = format!("Battery: {percentage:.0}% ({state})");
                let details = serde_json::json!({
                    "percentage": percentage,
                    "state": state,
                    "time_to_empty": time_to_empty,
                    "time_to_full": time_to_full,
                });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            EventPayload::NetworkStatus { connected, ssid, strength } => {
                let summary = if *connected {
                    format!("Network: connected{}", ssid.as_ref().map(|s| format!(" to {s}")).unwrap_or_default())
                } else {
                    "Network: disconnected".to_string()
                };
                let details = serde_json::json!({
                    "connected": connected,
                    "ssid": ssid,
                    "strength": strength,
                });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            // Audio/media → memory.
            EventPayload::MediaPlayback { player, state, title, artist } => {
                let media_info = title.as_ref()
                    .map(|t| format!("{player}: {t}{}", artist.as_ref().map(|a| format!(" by {a}")).unwrap_or_default()))
                    .unwrap_or(player.clone());
                let summary = format!("Media {state}: {media_info}");
                let details = serde_json::json!({
                    "player": player,
                    "state": state,
                    "title": title,
                    "artist": artist,
                });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            // Clipboard → memory (preview only for privacy).
            EventPayload::ClipboardUpdated { content_type, preview } => {
                let summary = format!("Clipboard ({content_type}): {preview}");
                let details = serde_json::json!({
                    "type": content_type,
                    "preview": preview,
                });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            // Action lifecycle → action memory.
            EventPayload::ActionStarted { message, .. } => {
                let _ = self.store.insert(MemoryType::Action, workspace, message, &serde_json::json!({}));
            }

            EventPayload::ActionCompleted { result, .. } => {
                let summary = format!("Completed: {result}");
                let details = serde_json::json!({ "result": result });
                let _ = self.store.insert(MemoryType::Action, workspace, &summary, &details);
            }

            EventPayload::ActionFailed { error, .. } => {
                let summary = format!("Failed: {error}");
                let details = serde_json::json!({ "error": error });
                let _ = self.store.insert(MemoryType::Action, workspace, &summary, &details);
            }

            // Agent events → memory.
            EventPayload::AgentSpawned { agent_id, task } => {
                let summary = format!("Agent spawned: {task}");
                let details = serde_json::json!({ "agent_id": agent_id, "task": task });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            EventPayload::AgentCompleted { agent_id, result } => {
                let summary = format!("Agent completed: {result}");
                let details = serde_json::json!({ "agent_id": agent_id, "result": result });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            EventPayload::AgentFailed { agent_id, error } => {
                let summary = format!("Agent failed: {error}");
                let details = serde_json::json!({ "agent_id": agent_id, "error": error });
                let _ = self.store.insert(MemoryType::Event, workspace, &summary, &details);
            }

            _ => {
                // Ignore other event types for memory.
            }
        }
    }

    /// Record a user intent (query to AI runtime).
    pub fn record_intent(&self, query: &str, context: &serde_json::Value) {
        let summary = query.to_string();
        let details = serde_json::json!({
            "query": query,
            "context": context,
        });
        let _ = self.store.insert(MemoryType::Intent, None, &summary, &details);
    }

    /// Record an AI response.
    pub fn record_ai_response(&self, query: &str, response: &str) {
        let summary = format!("Q: {query}");
        let details = serde_json::json!({
            "query": query,
            "response": response,
        });
        let _ = self.store.insert(MemoryType::AiResponse, None, &summary, &details);
    }

    /// Record a context snapshot.
    pub fn record_context_snapshot(&self, snapshot: &serde_json::Value) {
        let summary = "Context snapshot".to_string();
        let _ = self.store.insert(MemoryType::ContextSnapshot, None, &summary, snapshot);
    }
}
