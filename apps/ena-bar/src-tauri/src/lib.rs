mod ipc_client;
mod commands;

/// Start the IPC client background task that connects to enad.
/// Forwards events from enad to the Tauri frontend.
fn start_ipc_client(app: &tauri::AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Wait a moment for enad to be ready, then connect.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if let Err(e) = ipc_client::connect_and_listen(handle).await {
            tracing::warn!("IPC client failed: {e}");
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Spawn the IPC client to connect to enad.
            start_ipc_client(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping_enad,
            commands::query_system_info,
            commands::spawn_agent,
            commands::execute_command,
            commands::get_context,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ena-bar");
}
