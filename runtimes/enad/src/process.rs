use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

/// A tracked process managed by enad.
struct TrackedProcess {
    id: Uuid,
    pid: u32,
    command: String,
    child: Child,
}

/// Process lifecycle manager.
/// Spawns, tracks, and terminates child processes.
/// Emits events on the bus for lifecycle changes.
pub struct ProcessManager {
    processes: Mutex<HashMap<Uuid, TrackedProcess>>,
    bus: Arc<EventBus>,
}

impl ProcessManager {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
            bus,
        }
    }

    /// Spawn a new process and begin tracking it.
    pub async fn spawn(&self, command: &str, args: &[String]) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();

        let child = Command::new(command)
            .args(args)
            .kill_on_drop(true)
            .spawn()?;

        let pid = child
            .id()
            .expect("freshly spawned process should have a pid");
        info!("Spawned process {id}: {command} (pid {pid})");

        self.bus.publish(SystemEvent::new(
            "process",
            EventKind::Process,
            EventPayload::ProcessStarted {
                pid,
                command: command.to_string(),
            },
        ));

        let tracked = TrackedProcess {
            id,
            pid,
            command: command.to_string(),
            child,
        };

        self.processes.lock().await.insert(id, tracked);
        Ok(id)
    }

    /// Request termination of a tracked process.
    /// Releases the lock before calling kill() to avoid blocking the mutex.
    pub async fn terminate(&self, id: Uuid) -> anyhow::Result<()> {
        let tracked = {
            let mut processes = self.processes.lock().await;
            processes.remove(&id)
        };

        if let Some(mut tracked) = tracked {
            tracked.child.kill().await?;
            info!("Terminated process {id} (pid {})", tracked.pid);

            self.bus.publish(SystemEvent::new(
                "process",
                EventKind::Process,
                EventPayload::ProcessExited {
                    pid: tracked.pid,
                    exit_code: -1,
                },
            ));
        } else {
            anyhow::bail!("No tracked process with id {id}");
        }
        Ok(())
    }

    /// List all currently tracked process IDs.
    pub async fn list(&self) -> Vec<(Uuid, u32, String)> {
        let processes = self.processes.lock().await;
        processes
            .values()
            .map(|p| (p.id, p.pid, p.command.clone()))
            .collect()
    }

    /// Check for exited processes and remove them from tracking.
    pub async fn reap_zombies(&self) {
        let mut processes = self.processes.lock().await;
        let mut to_remove = Vec::new();

        // Use iter_mut() to get mutable access to child for try_wait().
        for (id, tracked) in processes.iter_mut() {
            match tracked.child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(-1);
                    info!("Process {id} exited with code {code}");

                    self.bus.publish(SystemEvent::new(
                        "process",
                        EventKind::Process,
                        EventPayload::ProcessExited {
                            pid: tracked.pid,
                            exit_code: code,
                        },
                    ));

                    to_remove.push(*id);
                }
                Ok(None) => {
                    // Still running.
                }
                Err(e) => {
                    warn!("Error checking process {id}: {e}");
                    to_remove.push(*id);
                }
            }
        }

        for id in to_remove {
            processes.remove(&id);
        }
    }
}
