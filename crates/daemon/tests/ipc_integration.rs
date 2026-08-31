use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use daemon::request_handler::handle_request;

use history::{ClipboardHistoryService, HistoryConfig};

use ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, ServerError};

use storage::{Database, StorageRepository};

use chrono::{Duration as ChronoDuration, Utc};

use pookie_clipboard::ClipboardContent;
use pookie_core::ClipboardItem;
use uuid::Uuid;

static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temporary_socket_path() -> PathBuf {
    let counter = SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed);

    std::env::temp_dir().join(format!(
        "pookie-paste-daemon-test-{}-{}.sock",
        std::process::id(),
        counter,
    ))
}

struct TestIpcApp {
    socket_path: PathBuf,
    server_task: tokio::task::JoinHandle<()>,
    history_service: Arc<ClipboardHistoryService>,
}

impl TestIpcApp {
    async fn start() -> Self {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let history_service = Arc::new(ClipboardHistoryService::new(
            repository,
            HistoryConfig { max_items: 30 },
        ));

        let socket_path = temporary_socket_path();

        let server = IpcServer::bind(&socket_path).expect("IPC server bind failed");

        let service = Arc::clone(&history_service);

        let server_task = tokio::spawn(async move {
            loop {
                let connection = match server.accept().await {
                    Ok(connection) => connection,

                    Err(_) => break,
                };

                let service = Arc::clone(&service);

                tokio::spawn(async move {
                    handle_test_connection(connection, service).await;
                });
            }
        });

        Self {
            socket_path,
            server_task,
            history_service,
        }
    }

    async fn client(&self) -> IpcClient {
        IpcClient::connect(&self.socket_path)
            .await
            .expect("IPC client connection failed")
    }

    fn history_service(&self) -> Arc<ClipboardHistoryService> {
        Arc::clone(&self.history_service)
    }

    fn socket_path(&self) -> PathBuf {
        self.socket_path.clone()
    }
}

async fn handle_test_connection(
    mut connection: ipc::IpcConnection,
    history_service: Arc<ClipboardHistoryService>,
) {
    loop {
        let request = match connection.read_request().await {
            Ok(request) => request,

            Err(ServerError::ConnectionClosed) => {
                break;
            }

            Err(_) => {
                break;
            }
        };

        let response = handle_request(request, history_service.as_ref()).await;

        if connection.send_response(&response).await.is_err() {
            break;
        }
    }
}

impl Drop for TestIpcApp {
    fn drop(&mut self) {
        self.server_task.abort();

        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn test_item(text: &str, hash: &str, offset_seconds: i64) -> ClipboardItem {
    ClipboardItem {
        id: Uuid::new_v4(),
        content: ClipboardContent::Text(text.to_string()),
        hash: hash.to_string(),
        created_at: Utc::now() + ChronoDuration::seconds(offset_seconds),
    }
}

#[tokio::test]
async fn ping_round_trips_through_daemon_ipc_stack() {
    let app = TestIpcApp::start().await;

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::Ping)
        .await
        .expect("Ping request failed");

    assert_eq!(response, IpcResponse::Pong,);
}

#[tokio::test]
async fn get_history_returns_items_newest_first() {
    let app = TestIpcApp::start().await;

    let service = app.history_service();

    service
        .save(test_item("First", "hash-first", 1))
        .await
        .expect("first save failed");

    service
        .save(test_item("Second", "hash-second", 2))
        .await
        .expect("second save failed");

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::GetHistory)
        .await
        .expect("GetHistory request failed");

    match response {
        IpcResponse::History { items } => {
            assert_eq!(items.len(), 2);

            assert_eq!(items[0].text_content.as_deref(), Some("Second"),);

            assert_eq!(items[1].text_content.as_deref(), Some("First"),);
        }

        other => {
            panic!("unexpected response: {other:?}");
        }
    }
}

#[tokio::test]
async fn delete_item_removes_history_entry() {
    let app = TestIpcApp::start().await;

    let service = app.history_service();

    let item = test_item("Delete me", "delete-hash", 1);

    let id = item.id.to_string();

    service.save(item).await.expect("save failed");

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::DeleteItem { id: id.clone() })
        .await
        .expect("DeleteItem request failed");

    assert_eq!(response, IpcResponse::Deleted { deleted: true },);

    let items = service.get_all().await.expect("history retrieval failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn delete_item_returns_false_for_missing_entry() {
    let app = TestIpcApp::start().await;

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::DeleteItem {
            id: "missing-item".to_string(),
        })
        .await
        .expect("DeleteItem request failed");

    assert_eq!(response, IpcResponse::Deleted { deleted: false },);
}

#[tokio::test]
async fn clear_history_removes_all_entries() {
    let app = TestIpcApp::start().await;

    let service = app.history_service();

    for index in 1..=3 {
        service
            .save(test_item(
                &format!("Item {index}"),
                &format!("hash-{index}"),
                index,
            ))
            .await
            .expect("save failed");
    }

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::ClearHistory)
        .await
        .expect("ClearHistory request failed");

    assert_eq!(response, IpcResponse::Cleared { count: 3 },);

    let items = service.get_all().await.expect("history retrieval failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn clear_history_returns_zero_when_empty() {
    let app = TestIpcApp::start().await;

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::ClearHistory)
        .await
        .expect("ClearHistory request failed");

    assert_eq!(response, IpcResponse::Cleared { count: 0 },);
}

#[tokio::test]
async fn supports_multiple_requests_on_same_connection() {
    let app = TestIpcApp::start().await;

    let service = app.history_service();

    let item = test_item("Persistent session item", "persistent-session-hash", 1);

    let item_id = item.id.to_string();

    service.save(item).await.expect("save failed");

    let mut client = app.client().await;

    let ping = client.send(&IpcRequest::Ping).await.expect("Ping failed");

    assert_eq!(ping, IpcResponse::Pong,);

    let history = client
        .send(&IpcRequest::GetHistory)
        .await
        .expect("GetHistory failed");

    match history {
        IpcResponse::History { items } => {
            assert_eq!(items.len(), 1,);
        }

        other => {
            panic!("unexpected response: {other:?}");
        }
    }

    let deleted = client
        .send(&IpcRequest::DeleteItem { id: item_id })
        .await
        .expect("DeleteItem failed");

    assert_eq!(deleted, IpcResponse::Deleted { deleted: true },);

    let history = client
        .send(&IpcRequest::GetHistory)
        .await
        .expect("second GetHistory failed");

    match history {
        IpcResponse::History { items } => {
            assert!(items.is_empty());
        }

        other => {
            panic!("unexpected response: {other:?}");
        }
    }
}

#[tokio::test]
async fn handles_multiple_clients_concurrently() {
    let app = TestIpcApp::start().await;

    let service = app.history_service();

    service
        .save(test_item("Concurrent item", "concurrent-hash", 1))
        .await
        .expect("save failed");

    let first_path = app.socket_path();

    let second_path = app.socket_path();

    let third_path = app.socket_path();

    let first = tokio::spawn(async move {
        let mut client = IpcClient::connect(first_path)
            .await
            .expect("first connection failed");

        client
            .send(&IpcRequest::Ping)
            .await
            .expect("first request failed")
    });

    let second = tokio::spawn(async move {
        let mut client = IpcClient::connect(second_path)
            .await
            .expect("second connection failed");

        client
            .send(&IpcRequest::GetHistory)
            .await
            .expect("second request failed")
    });

    let third = tokio::spawn(async move {
        let mut client = IpcClient::connect(third_path)
            .await
            .expect("third connection failed");

        client
            .send(&IpcRequest::Ping)
            .await
            .expect("third request failed")
    });

    let (first, second, third) = tokio::join!(first, second, third,);

    assert_eq!(first.unwrap(), IpcResponse::Pong,);

    match second.unwrap() {
        IpcResponse::History { items } => {
            assert_eq!(items.len(), 1,);
        }

        other => {
            panic!("unexpected response: {other:?}");
        }
    }

    assert_eq!(third.unwrap(), IpcResponse::Pong,);
}

#[tokio::test]
async fn handles_concurrent_read_and_delete() {
    let app = TestIpcApp::start().await;

    let service = app.history_service();

    let item = test_item("Concurrent delete item", "concurrent-delete-hash", 1);

    let item_id = item.id.to_string();

    service.save(item).await.expect("save failed");

    let read_path = app.socket_path();

    let delete_path = app.socket_path();

    let read_task = tokio::spawn(async move {
        let mut client = IpcClient::connect(read_path)
            .await
            .expect("read client failed");

        client.send(&IpcRequest::GetHistory).await
    });

    let delete_task = tokio::spawn(async move {
        let mut client = IpcClient::connect(delete_path)
            .await
            .expect("delete client failed");

        client.send(&IpcRequest::DeleteItem { id: item_id }).await
    });

    let (read_result, delete_result) = tokio::join!(read_task, delete_task,);

    assert!(read_result.unwrap().is_ok());

    assert_eq!(
        delete_result.unwrap().expect("delete request failed",),
        IpcResponse::Deleted { deleted: true },
    );

    let items = service.get_all().await.expect("final history read failed");

    assert!(items.is_empty());
}
