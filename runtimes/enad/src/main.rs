mod actions;
mod bus;
mod first_run;
mod hooks;
mod memory;
mod orchestration;
mod process;
mod restore;
mod server;
mod snapshot;
mod context;
mod suggestion;
mod types;

#[cfg(target_os = "linux")]
mod system;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use clap::Parser;
use tokio::sync::broadcast;
use tracing::{error, info};

/// EnaOS Core System Daemon.
/// Manages IPC, event bus, process lifecycle, and system hooks.
#[derive(Parser, Debug)]
#[command(name = "enad", version, about)]
struct Cli {
    /// Path to the Unix domain socket.
    #[arg(long, default_value = "/tmp/enad.sock")]
    socket: String,

    /// Enable desktop integration subsystems.
    #[arg(long, default_value = "true")]
    desktop_integration: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ──
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "enad=info".into()),
        )
        .init();

    // ── CLI ──
    let cli = Cli::parse();
    info!("enad v{} starting up", env!("CARGO_PKG_VERSION"));

    // ── Core subsystems ──
    let bus = Arc::new(bus::EventBus::default());
    let process_manager = Arc::new(process::ProcessManager::new(bus.clone()));
    let action_executor = Arc::new(actions::executor::ActionExecutor::new(bus.clone()));
    let orchestration = Arc::new(orchestration::engine::OrchestrationEngine::new(
        bus.clone(),
        action_executor.clone(),
    ));
    let system_hooks = hooks::SystemHooks::new(bus.clone());

    // ── Memory subsystem ──
    let memory_path = format!("{}/ena-memory.db", std::env::temp_dir().display());
    let memory_store = match memory::store::MemoryStore::open(&memory_path) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::warn!("Memory store failed to open: {e} — memory disabled");
            Arc::new(memory::store::MemoryStore::open("/tmp/ena-memory.db").unwrap_or_else(|_| {
                // Fallback: in-memory-like behavior with a tmp file.
                panic!("Cannot initialize memory store: {e}");
            }))
        }
    };
    let memory_capture = memory::capture::MemoryCapture::new(memory_store.clone());

    // ── Workspace Snapshot subsystem ──
    let snapshot_path = format!("{}/ena-snapshots.db", std::env::temp_dir().display());
    let snapshot_store = match snapshot::store::SnapshotStore::open(&snapshot_path) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::warn!("Snapshot store failed to open: {e} — snapshots disabled");
            Arc::new(snapshot::store::SnapshotStore::open("/tmp/ena-snapshots.db").unwrap_or_else(|_| {
                panic!("Cannot initialize snapshot store: {e}");
            }))
        }
    };
    let snapshot_capture = snapshot::capture::SnapshotCapture::new(
        snapshot_store.clone(),
        memory_store.clone(),
        orchestration.clone(),
    );

    // ── Ambient suggestion subsystem ──
    let suggestion_path = format!("{}/ena-suggestions.db", std::env::temp_dir().display());
    let suggestion_store = match suggestion::store::SuggestionStore::open(&suggestion_path) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::warn!("Suggestion store failed to open: {e} — suggestions disabled");
            Arc::new(suggestion::store::SuggestionStore::open("/tmp/ena-suggestions.db").unwrap_or_else(|_| {
                panic!("Cannot initialize suggestion store: {e}");
            }))
        }
    };
    let suggestion_engine = Arc::new(suggestion::engine::SuggestionEngine::new(
        suggestion_store.clone(),
        bus.clone(),
    ));

    // ── Contextual Command Intelligence ──
    let context_engine = Arc::new(context::ContextEngine::new());

    // ── First-run manager + demo seeding ──
    let mem_path = std::env::temp_dir().join("ena-memory.db").exists();
    let snap_path = std::env::temp_dir().join("ena-snapshots.db").exists();
    let has_existing_db = mem_path || snap_path;
    let data_dir = std::env::temp_dir().to_string_lossy().to_string();
    let first_run_manager = Arc::new(first_run::FirstRunManager::new(&data_dir, has_existing_db));

    if first_run_manager.is_first_launch() {
        info!("Seeding demo data for first launch");
        // Demo snapshot is served on-demand via GetDemoData IPC command,
        // not persisted to the real snapshot store.
        first_run_manager.mark_demo_seeded();
    }

    // ── IPC server ──
    let server = server::IpcServer::bind(
        &cli.socket, bus.clone(), action_executor.clone(), memory_store.clone(), snapshot_store.clone(), orchestration.clone(), suggestion_engine.clone(), context_engine.clone(), first_run_manager.clone(),
    )?;
    let shutdown_handle = server.shutdown_handle();

    // ── Spawn subsystems ──
    let server_handle = tokio::spawn(async move {
        server.run().await;
    });

    let process_reaper = {
        let pm = process_manager.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                pm.reap_zombies().await;
            }
        })
    };

    // ── Desktop integration subsystems ──
    #[cfg(target_os = "linux")]
    let desktop_handles = if cli.desktop_integration {
        info!("Desktop integration: enabled");
        let mut handles = Vec::new();

        // UPower battery watcher.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::upower::run(b).await;
            }));
        }

        // NetworkManager connectivity watcher.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::network::run(b).await;
            }));
        }

        // Window focus tracker.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::window::run(b).await;
            }));
        }

        // Workspace awareness.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::workspace::run(b).await;
            }));
        }

        // Clipboard monitor.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::clipboard::run(b).await;
            }));
        }

        // Notification listener.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::notifications::run(b).await;
            }));
        }

        // Audio state watcher.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::audio::run(b).await;
            }));
        }

        // MPRIS media playback watcher.
        {
            let b = bus.clone();
            handles.push(tokio::spawn(async move {
                system::audio::run_mpris(b).await;
            }));
        }

        Some(handles)
    } else {
        info!("Desktop integration: disabled");
        None
    };

    #[cfg(not(target_os = "linux"))]
    let desktop_handles: Option<Vec<tokio::task::JoinHandle<()>>> = None;

    // ── Memory capture ──
    let memory_capture_handle = {
        let mc = memory_capture;
        let b = bus.clone();
        tokio::spawn(async move {
            mc.run(b).await;
        })
    };

    // ── Snapshot capture (auto-snapshot loop) ──
    let snapshot_capture_handle = {
        let sc = snapshot_capture;
        let b = bus.clone();
        tokio::spawn(async move {
            sc.run(b).await;
        })
    };

    // ── Ambient suggestion engine (event-driven) ──
    // Subscribe to the event bus and feed events to the engine.
    let suggestion_engine_clone = suggestion_engine.clone();
    let suggestion_bus = bus.clone();
    let suggestion_task = tokio::spawn(async move {
        let mut rx = suggestion_bus.subscribe_all();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    suggestion_engine_clone.on_event(&event);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Suggestion engine lagged by {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Context engine event subscription — feeds events to the aggregator.
    let context_clone = context_engine.clone();
    let context_bus = bus.clone();
    let context_event_task = tokio::spawn(async move {
        let mut rx = context_bus.subscribe_all();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let kind = serde_json::to_value(&event.kind).ok().and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_default();
                    let payload = serde_json::to_value(&event.payload).unwrap_or_default();
                    context_clone.on_event(&kind, &payload);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Context engine lagged by {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Periodic context refresh — pulls deep state from stores.
    let context_refresh = context_engine.clone();
    let ctx_mem = memory_store.clone();
    let ctx_orch = orchestration.clone();
    let ctx_snaps = snapshot_store.clone();
    let context_refresh_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;

            // Pull recent intents from memory.
            let intents_q = crate::memory::types::MemoryQuery::intents();
            let recent_intents: Vec<String> = ctx_mem.query(&intents_q)
                .ok()
                .map(|entries| entries.iter().map(|e| e.summary.clone()).take(10).collect())
                .unwrap_or_default();

            let actions_q = crate::memory::types::MemoryQuery::actions();
            let recent_actions: Vec<String> = ctx_mem.query(&actions_q)
                .ok()
                .map(|entries| entries.iter().map(|e| e.summary.clone()).take(5).collect())
                .unwrap_or_default();

            // Pull active plans.
            let plans = ctx_orch.list_plans().await;
            use crate::orchestration::types::PlanStatus;
            let active_plans: Vec<crate::context::aggregator::ActivePlan> = plans
                .into_iter()
                .filter(|p| matches!(p.status, PlanStatus::PendingApproval | PlanStatus::Running))
                .map(|p| crate::context::aggregator::ActivePlan {
                    id: p.id.to_string(),
                    title: p.title.clone(),
                    status: format!("{:?}", p.status),
                })
                .take(5)
                .collect();

            // Pull recent snapshots.
            let recent_snapshots: Vec<crate::context::aggregator::RecentSnapshot> = ctx_snaps.list(5)
                .ok()
                .map(|snaps| snaps.into_iter().map(|s| crate::context::aggregator::RecentSnapshot {
                    id: s.snapshot_id.to_string(),
                    label: s.label.clone(),
                    taken_at: s.created_at.to_rfc3339(),
                }).collect())
                .unwrap_or_default();

            context_refresh.refresh(
                recent_intents,
                recent_actions,
                active_plans,
                recent_snapshots,
            );
        }
    });

    // Periodic cleanup of expired suggestions and dismissal records.
    let suggestion_cleanup = suggestion_engine.clone();
    let cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            suggestion_cleanup.cleanup();
        }
    });

    // ── Install system hooks ──
    let hooks_shutdown = system_hooks.wait_for_shutdown();

    // ── Emit startup event ──
    bus.publish(types::events::SystemEvent::new(
        "enad",
        types::events::EventKind::System,
        types::events::EventPayload::SystemActive,
    ));

    info!("enad ready — awaiting commands on {}", cli.socket);

    // ── Wait for shutdown signal ──
    hooks_shutdown.await;

    // ── Graceful shutdown ──
    info!("Shutting down enad...");

    // Signal servers to stop.
    let _ = shutdown_handle.send(());

    // Wait for server to finish (with timeout).
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let _ = server_handle.await;
        let _ = process_reaper.await;
        memory_capture_handle.abort();
        snapshot_capture_handle.abort();
        suggestion_task.abort();
        cleanup_task.abort();
        context_event_task.abort();
        context_refresh_task.abort();

        // Abort desktop integration tasks.
        if let Some(handles) = desktop_handles {
            for h in handles {
                h.abort();
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        error!("Shutdown timed out — forcing exit");
    });

    // Clean up socket.
    let _ = std::fs::remove_file(&cli.socket);

    info!("enad stopped");
    Ok(())
}
