mod actions;
mod bus;
mod hooks;
mod process;
mod server;
mod types;

#[cfg(target_os = "linux")]
mod system;

use std::sync::Arc;

use clap::Parser;
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
    let system_hooks = hooks::SystemHooks::new(bus.clone());

    // ── IPC server ──
    let server = server::IpcServer::bind(&cli.socket, bus.clone(), action_executor.clone())?;
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
