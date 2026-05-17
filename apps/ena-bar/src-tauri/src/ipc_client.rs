use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info, warn};

/// The path to enad's Unix domain socket.
const ENAD_SOCKET_PATH: &str = "/tmp/enad.sock";

/// Internal state: the write half of the connection, protected by a Mutex.
pub struct EnadConnection {
    writer: Mutex<tokio::io::WriteHalf<UnixStream>>,
}

/// Connect to enad and listen for events, forwarding them to the Tauri frontend.
pub async fn connect_and_listen(app_handle: AppHandle) -> anyhow::Result<()> {
    info!("Connecting to enad at {ENAD_SOCKET_PATH}...");

    let stream = UnixStream::connect(ENAD_SOCKET_PATH).await?;
    let (reader, writer) = tokio::io::split(stream);

    // Store the write half so commands can use it later.
    let connection = Arc::new(EnadConnection {
        writer: Mutex::new(writer),
    });
    app_handle.manage(connection);

    // Send a subscription to all events.
    {
        let msg = serde_json::json!({
            "id": uuid::Uuid::new_v4(),
            "kind": {
                "type": "Subscribe",
                "body": {
                    "kinds": []
                }
            }
        });
        let guard = app_handle.state::<Arc<EnadConnection>>();
        let mut writer = guard.writer.lock().await;
        let line = serde_json::to_string(&msg)?;
        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    info!("Connected to enad, listening for events...");

    // Read events in a loop and emit them to the frontend.
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                info!("enad disconnected");
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<serde_json::Value>(trimmed) {
                    Ok(value) => {
                        // Forward as a Tauri event to the frontend.
                        let _ = app_handle.emit("enad-event", value);
                    }
                    Err(e) => {
                        warn!("Failed to parse enad message: {e}");
                    }
                }
            }
            Err(e) => {
                error!("Read error from enad: {e}");
                break;
            }
        }
    }

    Ok(())
}

/// Send a raw JSON message to enad over the Unix socket.
pub async fn send_message(app_handle: &AppHandle, message: &serde_json::Value) -> anyhow::Result<()> {
    let guard = app_handle.state::<Arc<EnadConnection>>();
    let mut writer = guard.writer.lock().await;
    let line = serde_json::to_string(message)?;
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
