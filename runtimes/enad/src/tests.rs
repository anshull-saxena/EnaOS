/// Integration tests for enad IPC.
///
/// Uses tokio's async UnixStream (matching the server) to avoid
/// the EAGAIN/EWOULDBLOCK issue that occurs when mixing
/// std::os::unix::net with tokio's async I/O on macOS.
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use uuid::Uuid;

use crate::actions::executor::ActionExecutor;
use crate::bus::EventBus;
use crate::context::ContextEngine;
use crate::first_run::FirstRunManager;
use crate::memory::store::MemoryStore;
use crate::orchestration::engine::OrchestrationEngine;
use crate::server::IpcServer;
use crate::snapshot::store::SnapshotStore;
use crate::suggestion::engine::SuggestionEngine;
use crate::suggestion::store::SuggestionStore;

/// Path for a temporary Unix socket used by tests.
fn test_socket_path(test_name: &str) -> String {
    format!("/tmp/ena-test-{}.sock", test_name)
}

/// Send a command and read the response using tokio's async UnixStream.
async fn send_command_raw(socket_path: &str, json: &Value) -> Result<Value, String> {
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| format!("connect: {e}"))?;

    let (mut reader, mut writer) = stream.into_split();

    let msg = serde_json::to_string(json).map_err(|e| format!("serialize: {e}"))?;
    writer
        .write_all(msg.as_bytes())
        .await
        .map_err(|e| format!("write: {e}"))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|e| format!("write nl: {e}"))?;
    writer.flush().await.map_err(|e| format!("flush: {e}"))?;

    let mut line = String::new();
    let mut buf_reader = BufReader::new(&mut reader);
    buf_reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("read: {e}"))?;

    if line.is_empty() {
        return Err("EOF — server closed connection".into());
    }

    serde_json::from_str(&line).map_err(|e| format!("parse: {e}"))
}

/// Build a unit command envelope (e.g. GetFirstRunStatus).
fn unit_command(name: &str) -> Value {
    serde_json::json!({
        "id": Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": name
        }
    })
}

/// Build a struct command envelope (e.g. GetContextCommands).
fn struct_command(name: &str, body: Value) -> Value {
    serde_json::json!({
        "id": Uuid::new_v4(),
        "kind": {
            "type": "Command",
            "body": {
                name: body
            }
        }
    })
}

/// Create a minimal enad server for testing, bound to a temp socket.
fn setup_test_server(test_name: &str) -> (String, IpcServer) {
    let socket = test_socket_path(test_name);
    let _ = std::fs::remove_file(&socket);

    let bus = Arc::new(EventBus::new(64));
    let temp_dir = std::env::temp_dir();
    let data_dir = temp_dir.join("ena-test").join(test_name);
    // Clean any leftover first-run marker from a previous test run.
    let marker = data_dir.join(".ena-first-run-completed");
    let _ = std::fs::remove_file(&marker);
    let _ = std::fs::create_dir_all(&data_dir);

    let db_path = data_dir.join("test.db");
    let snapshot_store =
        Arc::new(SnapshotStore::open(db_path.to_str().unwrap()).expect("open snapshot store"));
    let memory_store =
        Arc::new(MemoryStore::open(db_path.to_str().unwrap()).expect("open memory store"));
    let suggestion_store = Arc::new(
        SuggestionStore::open(data_dir.join("suggestions.db").to_str().unwrap())
            .expect("open suggestion store"),
    );
    let action_executor = Arc::new(ActionExecutor::new(bus.clone()));
    let orchestration = Arc::new(OrchestrationEngine::new(
        bus.clone(),
        action_executor.clone(),
    ));
    let suggestion_engine = Arc::new(SuggestionEngine::new(suggestion_store.clone(), bus.clone()));
    let context_engine = Arc::new(ContextEngine::new());
    let first_run_manager = Arc::new(FirstRunManager::new(
        data_dir.to_str().unwrap(),
        false, // has_db = false → fresh install
    ));

    let server = IpcServer::bind(
        &socket,
        bus,
        action_executor,
        memory_store,
        snapshot_store,
        orchestration,
        suggestion_engine,
        context_engine,
        first_run_manager,
    )
    .expect("bind IPC server");

    (socket, server)
}

/// Test that a Ping command gets a Pong response.
#[tokio::test]
async fn test_ping_pong() {
    let (socket, server) = setup_test_server("ping_pong");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });

    // Give the server a moment to start listening.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let ping = serde_json::json!({
        "id": Uuid::new_v4(),
        "kind": { "type": "Ping", "body": null }
    });
    let response = send_command_raw(&socket, &ping)
        .await
        .expect("ping request");

    let kind = response.get("kind").expect("response has kind");
    assert_eq!(kind.get("type").unwrap(), "Response");
    let body = kind.get("body").unwrap();
    let data = body.get("Data").expect("Response::Data variant");
    let payload = data.get("payload").unwrap();
    assert_eq!(payload.get("code").unwrap(), "PONG");
    assert!(payload.get("latency_ms").is_some());

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test GetFirstRunStatus returns fresh-install state.
#[tokio::test]
async fn test_first_run_status() {
    let (socket, server) = setup_test_server("first_run");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let cmd = unit_command("GetFirstRunStatus");
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("get first-run status");

    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let data = body.get("Data").expect("Response::Data variant");
    let payload = data.get("payload").unwrap();

    assert_eq!(payload.get("is_first_launch").unwrap(), true);
    assert_eq!(payload.get("onboarding_completed").unwrap(), false);
    assert!(payload.get("suggested_commands").is_some());

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test CompleteOnboarding transitions the state.
#[tokio::test]
async fn test_complete_onboarding() {
    let (socket, server) = setup_test_server("complete_onboarding");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify fresh install state first.
    let cmd = unit_command("GetFirstRunStatus");
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("get first-run status");
    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let data = body.get("Data").unwrap();
    let payload = data.get("payload").unwrap();
    assert_eq!(payload.get("is_first_launch").unwrap(), true);

    // Complete onboarding.
    let cmd = unit_command("CompleteOnboarding");
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("complete onboarding");
    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    assert!(body.get("Ok").is_some(), "Expected Response::Ok");

    // Verify onboarding is now marked completed.
    let cmd = unit_command("GetFirstRunStatus");
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("get first-run status");
    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let data = body.get("Data").unwrap();
    let payload = data.get("payload").unwrap();
    assert_eq!(payload.get("onboarding_completed").unwrap(), true);

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test GetContextCommands returns suggestions matching the query.
#[tokio::test]
async fn test_get_context_commands() {
    let (socket, server) = setup_test_server("context_commands");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let cmd = struct_command(
        "GetContextCommands",
        serde_json::json!({
            "query": "open browser",
            "limit": 6
        }),
    );
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("get context commands");

    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let data = body.get("Data").expect("Response::Data variant");
    let payload = data.get("payload").unwrap();

    assert!(
        payload.get("commands").is_some(),
        "must have 'commands' array"
    );
    assert!(
        payload.get("context").is_some(),
        "must have 'context' snapshot"
    );

    let commands = payload.get("commands").unwrap().as_array().unwrap();
    let labels: Vec<&str> = commands
        .iter()
        .filter_map(|c| c.get("label").and_then(|v| v.as_str()))
        .collect();
    assert!(
        labels.iter().any(|l| l.to_lowercase().contains("browser")),
        "Expected browser-related suggestion, got: {labels:?}"
    );

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test GetDemoData returns demo content.
#[tokio::test]
async fn test_get_demo_data() {
    let (socket, server) = setup_test_server("demo_data");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let cmd = unit_command("GetDemoData");
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("get demo data");

    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let data = body.get("Data").expect("Response::Data variant");
    let payload = data.get("payload").unwrap();

    assert_eq!(payload.get("demo").unwrap(), true);
    assert!(payload.get("snapshot").is_some(), "must have demo snapshot");
    assert!(
        payload.get("orchestration_plan").is_some(),
        "must have demo plan"
    );

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test ListSnapshots on an empty store (returns empty list).
#[tokio::test]
async fn test_list_snapshots_empty() {
    let (socket, server) = setup_test_server("list_empty");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let cmd = struct_command("ListSnapshots", serde_json::json!({ "limit": 10 }));
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("list snapshots");

    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let data = body.get("Data").expect("Response::Data variant");
    let payload = data.get("payload").unwrap();

    let arr = payload.as_array().expect("payload should be array");
    assert!(arr.is_empty(), "no snapshots yet");

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test DismissSuggestion on a non-existent ID (handles gracefully).
#[tokio::test]
async fn test_dismiss_nonexistent_suggestion() {
    let (socket, server) = setup_test_server("dismiss_missing");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let cmd = struct_command(
        "DismissSuggestion",
        serde_json::json!({
            "suggestion_id": Uuid::new_v4(),
            "permanent": false
        }),
    );
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("dismiss suggestion");

    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    assert!(body.get("Ok").is_some(), "Expected graceful Ok response");

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test that GetSuggestions returns the suggestions array.
#[tokio::test]
async fn test_get_suggestions() {
    let (socket, server) = setup_test_server("suggestions");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let cmd = struct_command("GetSuggestions", serde_json::json!({ "limit": 10 }));
    let response = send_command_raw(&socket, &cmd)
        .await
        .expect("get suggestions");

    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let data = body.get("Data").expect("Expected Response::Data");
    let payload = data.get("payload").unwrap();

    let suggestions = payload.get("suggestions").unwrap();
    assert!(suggestions.is_array(), "suggestions should be an array");

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test error response for malformed JSON using tokio async client.
#[tokio::test]
async fn test_malformed_json() {
    let (socket, server) = setup_test_server("malformed");
    let server_task = tokio::spawn(async move {
        server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let stream = UnixStream::connect(&socket).await.expect("connect");
    let (mut reader, mut writer) = stream.into_split();

    writer
        .write_all(b"NOT JSON\n")
        .await
        .expect("write garbage");
    writer.flush().await.expect("flush");

    let mut line = String::new();
    let mut buf_reader = BufReader::new(&mut reader);
    buf_reader
        .read_line(&mut line)
        .await
        .expect("read response");

    let response: Value = serde_json::from_str(&line).expect("parse response");
    let kind = response.get("kind").unwrap();
    let body = kind.get("body").unwrap();
    let error = body.get("Error").expect("Expected Error response");
    assert_eq!(error.get("code").unwrap(), "PARSE_ERROR");

    server_task.abort();
    let _ = std::fs::remove_file(&socket);
}

/// Test that an unavailable server produces a connection error.
#[tokio::test]
async fn test_server_unavailable() {
    let result = send_command_raw(
        "/tmp/nonexistent-test-socket.sock",
        &unit_command("GetFirstRunStatus"),
    )
    .await;
    match result {
        Err(e) => {
            assert!(e.contains("connect"), "Expected connection error, got: {e}");
        }
        Ok(_) => panic!("Expected error for unavailable server"),
    }
}
