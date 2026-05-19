/// ContextAggregator — cached desktop state aggregation.
///
/// Maintains a live snapshot of the desktop environment by consuming
/// events from the event bus. This is the single source of truth for
/// context-aware command resolution.
///
/// Key design: state is UPDATED on every event, not queried per keystroke.
/// This ensures sub-10ms latency for command resolution.
///
/// Deep state (memory entries, plans, snapshots) is refreshed
/// periodically by an external task via refresh_from_stores().

use std::sync::Mutex;

use serde_json::Value;

/// Cached desktop state.
#[derive(Debug, Clone, Default)]
pub struct DesktopState {
    pub focused_app: String,
    pub focused_title: String,
    pub workspace: String,
    pub battery_pct: f64,
    pub battery_state: String,
    pub network_connected: bool,
    pub network_ssid: String,
    pub clipboard_preview: String,
    pub media_player: String,
    pub media_title: String,
}

#[derive(Debug, Clone, Default)]
pub struct ActivePlan {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct RecentSnapshot {
    pub id: String,
    pub label: String,
    pub taken_at: String,
}

/// Aggregated context including live state + recent data.
#[derive(Debug, Clone, Default)]
pub struct AggregatedContext {
    pub desktop: DesktopState,
    pub recent_intents: Vec<String>,
    pub recent_actions: Vec<String>,
    pub active_plans: Vec<ActivePlan>,
    pub recent_snapshots: Vec<RecentSnapshot>,
}

pub struct ContextAggregator {
    state: Mutex<AggregatedContext>,
}

impl ContextAggregator {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(AggregatedContext::default()),
        }
    }

    /// Update cached state from an event.
    /// This is called on every event — no blocking, no I/O.
    pub fn update(&self, kind: &str, payload: &Value) {
        let mut ctx = self.state.lock().unwrap();
        let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let data = payload.get("data").cloned().unwrap_or(Value::Null);

        match kind {
            "Window" => match event_type {
                "WindowFocused" => {
                    if let Some(app) = data.get("app").and_then(|v| v.as_str()) {
                        ctx.desktop.focused_app = app.to_string();
                    }
                    if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                        ctx.desktop.focused_title = title.to_string();
                    }
                }
                "WorkspaceChanged" => {
                    if let Some(ws) = data.get("workspace").and_then(|v| v.as_str()) {
                        ctx.desktop.workspace = ws.to_string();
                    }
                }
                _ => {}
            },
            "System" => match event_type {
                "BatteryStatus" => {
                    if let Some(pct) = data.get("percentage").and_then(|v| v.as_f64()) {
                        ctx.desktop.battery_pct = pct;
                    }
                    if let Some(state) = data.get("state").and_then(|v| v.as_str()) {
                        ctx.desktop.battery_state = state.to_string();
                    }
                }
                "NetworkStatus" => {
                    if let Some(connected) = data.get("connected").and_then(|v| v.as_bool()) {
                        ctx.desktop.network_connected = connected;
                    }
                    if let Some(ssid) = data.get("ssid").and_then(|v| v.as_str()) {
                        ctx.desktop.network_ssid = ssid.to_string();
                    }
                }
                _ => {}
            },
            "Audio" => match event_type {
                "MediaPlayback" => {
                    if let Some(player) = data.get("player").and_then(|v| v.as_str()) {
                        ctx.desktop.media_player = player.to_string();
                    }
                    if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                        ctx.desktop.media_title = title.to_string();
                    }
                }
                _ => {}
            },
            "Clipboard" => match event_type {
                "ClipboardUpdated" => {
                    if let Some(preview) = data.get("preview").and_then(|v| v.as_str()) {
                        ctx.desktop.clipboard_preview = preview.to_string();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Refresh deep state from store data.
    /// Called periodically by an external async task.
    pub fn refresh_from_stores(
        &self,
        recent_intents: Vec<String>,
        recent_actions: Vec<String>,
        active_plans: Vec<ActivePlan>,
        recent_snapshots: Vec<RecentSnapshot>,
    ) {
        let mut ctx = self.state.lock().unwrap();
        ctx.recent_intents = recent_intents;
        ctx.recent_actions = recent_actions;
        ctx.active_plans = active_plans;
        ctx.recent_snapshots = recent_snapshots;
    }

    /// Get a snapshot of the current aggregated context.
    pub fn snapshot(&self) -> Value {
        let ctx = self.state.lock().unwrap();
        serde_json::json!({
            "focused_app": ctx.desktop.focused_app,
            "focused_title": ctx.desktop.focused_title,
            "workspace": ctx.desktop.workspace,
            "battery_pct": ctx.desktop.battery_pct,
            "battery_state": ctx.desktop.battery_state,
            "network_connected": ctx.desktop.network_connected,
            "network_ssid": ctx.desktop.network_ssid,
            "clipboard_preview": ctx.desktop.clipboard_preview,
            "media_player": ctx.desktop.media_player,
            "media_title": ctx.desktop.media_title,
            "recent_intents": ctx.recent_intents,
            "recent_actions": ctx.recent_actions,
            "active_plans": ctx.active_plans.iter().map(|p| serde_json::json!({
                "id": p.id,
                "title": p.title,
                "status": p.status,
            })).collect::<Vec<_>>(),
            "recent_snapshots": ctx.recent_snapshots.iter().map(|s| serde_json::json!({
                "id": s.id,
                "label": s.label,
                "taken_at": s.taken_at,
            })).collect::<Vec<_>>(),
        })
    }

    /// Get a reference to the current context for resolution.
    pub fn get_context(&self) -> AggregatedContext {
        self.state.lock().unwrap().clone()
    }
}
