mod actions;
mod bus;
mod hooks;
mod memory;
mod orchestration;
mod process;
mod restore;
mod server;
mod snapshot;
mod suggestion;
mod types;

#[cfg(target_os = "linux")]
mod system;

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

    // ── IPC server ──
    let server = server::IpcServer::bind(
        &cli.socket, bus.clone(), action_executor.clone(), memory_store.clone(), snapshot_store.clone(), orchestration.clone(), suggestion_engine.clone(),
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
