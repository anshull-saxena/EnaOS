use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

use crate::bus::EventBus;
use crate::memory::store::MemoryStore;
use crate::memory::types::MemoryQuery;
use crate::orchestration::engine::OrchestrationEngine;
use crate::snapshot::store::SnapshotStore;
use crate::snapshot::types::*;
use crate::types::events::*;

/// Captures workspace snapshots — auto-snapshot loop and event-driven capture.
pub struct SnapshotCapture {
    store: Arc<SnapshotStore>,
    memory: Arc<MemoryStore>,
    orchestration: Arc<OrchestrationEngine>,
}

impl SnapshotCapture {
    pub fn new(
        store: Arc<SnapshotStore>,
        memory: Arc<MemoryStore>,
        orchestration: Arc<OrchestrationEngine>,
    ) -> Self {
        Self {
            store,
            memory,
            orchestration,
        }
    }

    /// Start the background auto-snapshot loop (every 10 minutes) and
    /// event-driven snapshots (sleep/wake).
    pub async fn run(&self, bus: Arc<EventBus>) {
        info!("Snapshot capture: starting auto-snapshot loop");

        let mut rx = bus.subscribe_all();
        let last_checksum = Arc::new(std::sync::Mutex::new(String::new()));

        // ── Periodic auto-snapshot task ──
        let auto_store = self.store.clone();
        let auto_mem = self.memory.clone();
        let auto_orch = self.orchestration.clone();
        let auto_bus = bus.clone();
        let auto_ck = last_checksum.clone();
        let auto_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(600));
            loop {
                interval.tick().await;
                let snapshot = gather_snapshot(&auto_mem, &auto_orch, true).await;
                let ck = compute_checksum(&snapshot);

                let changed = {
                    let prev = auto_ck.lock().unwrap();
                    *prev != ck
                };
                if !changed {
                    continue;
                }

                *auto_ck.lock().unwrap() = ck;
                if let Err(e) = auto_store.insert(&snapshot) {
                    warn!("Auto-snapshot insert failed: {e}");
                } else {
                    info!(
                        "Auto-snapshot saved: {} ({} apps, {} terminals)",
                        snapshot.label,
                        snapshot.applications.len(),
                        snapshot.terminals.len()
                    );
                    let label = snapshot.label.clone();
                    let node_count = snapshot.node_count();
                    auto_bus.publish(SystemEvent::new(
                        "snapshot",
                        EventKind::System,
                        EventPayload::SnapshotTaken {
                            snapshot_id: snapshot.snapshot_id,
                            label,
                            node_count,
                        },
                    ));
                }

                let _ = auto_store.expire();
            }
        });

        // ── Event-driven snapshots ──
        loop {
            match rx.recv().await {
                Ok(event) => match &event.payload {
                    EventPayload::SystemSleep | EventPayload::SystemWake => {
                        let snapshot =
                            gather_snapshot(&self.memory, &self.orchestration, true).await;
                        let _ = self.store.insert(&snapshot);
                    }
                    _ => {}
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Snapshot capture: lagged by {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Snapshot capture: event bus closed");
                    break;
                }
            }
        }

        auto_task.abort();
    }

    /// Take an immediate manual snapshot.
    pub async fn take_snapshot(&self, label: &str) -> Result<Uuid, String> {
        let mut snapshot = gather_snapshot(&self.memory, &self.orchestration, false).await;
        if !label.is_empty() {
            snapshot.label = label.to_string();
        }
        snapshot.is_auto = false;
        let id = snapshot.snapshot_id;
        self.store.insert(&snapshot)?;
        Ok(id)
    }
}

/// Public helper: gather current environment state into a snapshot.
/// Used by both SnapshotCapture and the IPC command handler.
pub async fn gather_snapshot(
    memory: &MemoryStore,
    orchestration: &OrchestrationEngine,
    is_auto: bool,
) -> WorkspaceSnapshot {
    let label = if is_auto {
        let now = chrono::Utc::now();
        format!("Auto-snapshot {}", now.format("%H:%M"))
    } else {
        "Manual snapshot".to_string()
    };

    let mut snapshot = WorkspaceSnapshot::new(&label);
    snapshot.is_auto = is_auto;

    let mut mem_q = MemoryQuery::new();
    mem_q.limit = 30;
    if let Ok(entries) = memory.query(&mem_q) {
        for entry in &entries {
            match entry.entry_type {
                crate::memory::types::MemoryType::Event => {
                    if let Some(app) = entry.details.get("app").and_then(|v| v.as_str())
                        && !snapshot.applications.iter().any(|a| a.name == app)
                    {
                        snapshot.applications.push(AppInfo {
                            name: app.to_string(),
                            title: entry
                                .details
                                .get("title")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            pid: entry
                                .details
                                .get("pid")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32),
                            is_focused: false,
                        });
                    }
                }
                crate::memory::types::MemoryType::Action => {
                    snapshot.recent_actions.push(ActionRef {
                        action_id: Uuid::nil(),
                        action_type: "action".to_string(),
                        summary: entry.summary.clone(),
                        status: "completed".to_string(),
                        timestamp: entry.timestamp,
                    });
                }
                crate::memory::types::MemoryType::Intent => {
                    let query = entry
                        .details
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    snapshot.ai_conversations.push(ConversationRef {
                        query: query.to_string(),
                        response_summary: entry.summary.clone(),
                        timestamp: entry.timestamp,
                    });
                }
                crate::memory::types::MemoryType::AiResponse => {
                    let query = entry
                        .details
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !snapshot.ai_conversations.iter().any(|c| c.query == query) {
                        snapshot.ai_conversations.push(ConversationRef {
                            query: query.to_string(),
                            response_summary: entry.summary.clone(),
                            timestamp: entry.timestamp,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let plans = orchestration.list_plans().await;
    for plan in &plans {
        snapshot.orchestration_plans.push(OrchestrationPlanRef {
            plan_id: plan.id,
            title: plan.title.clone(),
            status: format!("{:?}", plan.status),
            created_at: plan.created_at,
        });
    }

    snapshot.env_checksum = compute_checksum(&snapshot);
    snapshot.active_project = detect_active_project(&snapshot);
    snapshot
}

/// Public helper: take an immediate snapshot and store it (IPC command handler).
pub async fn take_immediate_snapshot(
    store: &SnapshotStore,
    memory: &MemoryStore,
    orchestration: &OrchestrationEngine,
    label: &str,
    bus: &EventBus,
) -> Result<Uuid, String> {
    let mut snapshot = gather_snapshot(memory, orchestration, false).await;
    if !label.is_empty() {
        snapshot.label = label.to_string();
    }
    snapshot.is_auto = false;
    let id = snapshot.snapshot_id;
    store.insert(&snapshot)?;

    bus.publish(SystemEvent::new(
        "snapshot",
        EventKind::System,
        EventPayload::SnapshotTaken {
            snapshot_id: id,
            label: label.to_string(),
            node_count: snapshot.node_count(),
        },
    ));

    Ok(id)
}

fn compute_checksum(snapshot: &WorkspaceSnapshot) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for app in &snapshot.applications {
        app.name.hash(&mut hasher);
    }
    for ws in &snapshot.workspaces {
        ws.name.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn detect_active_project(snapshot: &WorkspaceSnapshot) -> Option<String> {
    for app in &snapshot.applications {
        if app.is_focused {
            let title = &app.title;
            if title.contains(" — ") {
                let parts: Vec<&str> = title.split(" — ").collect();
                if parts.len() == 2 && parts[1].contains("Code") {
                    return Some(parts[0].trim().to_string());
                }
            }
            if title.contains(" • ") {
                let parts: Vec<&str> = title.split(" • ").collect();
                if parts.len() >= 2 {
                    return Some(parts[parts.len() - 1].trim().to_string());
                }
            }
            let lower = app.name.to_lowercase();
            if (lower.contains("terminal")
                || lower.contains("kitty")
                || lower.contains("alacritty"))
                && !title.is_empty()
                && !title.contains("~")
            {
                return Some(title.trim().to_string());
            }
        }
    }
    for app in &snapshot.applications {
        let lower = app.name.to_lowercase();
        if (lower.contains("code")
            || lower.contains("idea")
            || lower.contains("vim")
            || lower.contains("neovim"))
            && !app.title.is_empty()
            && app.title != app.name
        {
            let parts: Vec<&str> = app.title.split(" — ").collect();
            return Some(parts[0].trim().to_string());
        }
    }
    None
}
