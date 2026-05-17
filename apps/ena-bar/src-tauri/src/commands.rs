use crate::ipc_client;
use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize)]
pub struct PingResult {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Ping enad to check connectivity.
#[tauri::command]
pub async fn ping_enad(app_handle: AppHandle) -> Result<PingResult, String> {
    let start = std::time::Instant::now();

    // Build a ping message matching enad's IPC protocol.
    let msg = serde_json::json!({
        "id": uuid::Uuid::new_v4(),
        "kind": "Ping"
    });

    ipc_client::send_message(&app_handle, &msg)
        .await
        .map_err(|e| format!("Failed to ping enad: {e}"))?;

    let elapsed = start.elapsed();
    Ok(PingResult {
        ok: true,
        latency_ms: Some(elapsed.as_millis() as u64),
        error: None,
    })
}

/// Query system information from enad.
#[tauri::command]
pub async fn query_system_info(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    let msg = serde_json::json!({
        "id": uuid::Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": {
                "type": "QueryState",
                "body": {
                    "target": "SystemInfo"
                }
            }
        }
    });

    ipc_client::send_message(&app_handle, &msg)
        .await
        .map_err(|e| format!("Failed to query enad: {e}"))?;

    // For now, return local machine info.
    // In the full implementation, we'd wait for enad's response.
    Ok(serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "hostname": hostname(),
        "version": "0.1.0",
    }))
}

/// Spawn an agent with a task description.
#[tauri::command]
pub async fn spawn_agent(app_handle: AppHandle, task: String, capabilities: Vec<String>) -> Result<serde_json::Value, String> {
    let msg = serde_json::json!({
        "id": uuid::Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": {
                "type": "SpawnAgent",
                "body": {
                    "task": task,
                    "capabilities": capabilities
                }
            }
        }
    });

    ipc_client::send_message(&app_handle, &msg)
        .await
        .map_err(|e| format!("Failed to send spawn command: {e}"))?;

    Ok(serde_json::json!({
        "status": "dispatched",
    }))
}

/// Execute a system command through enad.
#[tauri::command]
pub async fn execute_command(app_handle: AppHandle, command: String, args: Vec<String>) -> Result<serde_json::Value, String> {
    let msg = serde_json::json!({
        "id": uuid::Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": {
                "type": "Execute",
                "body": {
                    "command": command,
                    "args": args
                }
            }
        }
    });

    ipc_client::send_message(&app_handle, &msg)
        .await
        .map_err(|e| format!("Failed to send execute command: {e}"))?;

    Ok(serde_json::json!({
        "status": "dispatched",
    }))
}

/// Get system context for AI prompts.
#[tauri::command]
pub async fn get_context(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    let msg = serde_json::json!({
        "id": uuid::Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": {
                "type": "GetContext",
                "body": null
            }
        }
    });

    ipc_client::send_message(&app_handle, &msg)
        .await
        .map_err(|e| format!("Failed to get context: {e}"))?;

    Ok(serde_json::json!({
        "desktop": "EnaOS",
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "version": "0.1.0",
    }))
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .or_else(|_| {
            std::process::Command::new("hostname")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .map_err(|e: std::io::Error| e)
        })
        .unwrap_or_else(|_| "enaos".into())
}
