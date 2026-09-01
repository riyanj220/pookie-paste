use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use chrono::{Duration as ChronoDuration, Utc};

use daemon::activation_service::ClipboardActivationService;
use daemon::clipboard_service::ClipboardService;
use daemon::focus_backend::{FocusBackend, FocusError, FocusTarget};
use daemon::focus_service::FocusService;
use daemon::paste_backend::{ClipboardOnlyPasteBackend, PasteBackend, PasteCapability, PasteError};
use daemon::request_handler::handle_request;

use history::{ClipboardHistoryService, HistoryConfig};

use ipc::{ActivationOutcome, IpcClient, IpcRequest, IpcResponse, IpcServer, ServerError};

use pookie_clipboard::{ClipboardBackend, ClipboardContent, ClipboardError};

use pookie_core::ClipboardItem;

use storage::{Database, StorageRepository};

use tokio::sync::Mutex;

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

#[derive(Clone)]
struct FakeClipboardBackend {
    content: Arc<StdMutex<String>>,
}

impl FakeClipboardBackend {
    fn new(initial: &str) -> Self {
        Self {
            content: Arc::new(StdMutex::new(initial.to_string())),
        }
    }

    fn content(&self) -> String {
        self.content
            .lock()
            .expect("fake clipboard mutex poisoned")
            .clone()
    }
}

impl ClipboardBackend for FakeClipboardBackend {
    fn read(&self) -> Result<String, ClipboardError> {
        Ok(self
            .content
            .lock()
            .expect("fake clipboard mutex poisoned")
            .clone())
    }

    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        *self.content.lock().expect("fake clipboard mutex poisoned") = content.to_string();

        Ok(())
    }
}

/*
 * IPC integration tests do not need a real
 * window manager or X11 focus implementation.
 *
 * This backend immediately confirms focus.
 */
struct FakeFocusBackend {
    target_id: Option<u64>,
}

impl FocusBackend for FakeFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        match self.target_id {
            Some(id) => Ok(FocusTarget::new(id)),
            None => Err(FocusError::Unavailable),
        }
    }

    fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
        Ok(())
    }

    fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
        Ok(true)
    }
}

struct FakeDirectPasteBackend {
    pasted: Arc<AtomicBool>,
}

impl PasteBackend for FakeDirectPasteBackend {
    fn capability(&self) -> PasteCapability {
        PasteCapability::Direct
    }

    fn paste(&self) -> Result<(), PasteError> {
        self.pasted.store(true, Ordering::SeqCst);

        Ok(())
    }
}

type TestActivationService =
    ClipboardActivationService<FakeClipboardBackend, ClipboardOnlyPasteBackend, FakeFocusBackend>;

type X11TestActivationService =
    ClipboardActivationService<FakeClipboardBackend, FakeDirectPasteBackend, FakeFocusBackend>;

struct TestIpcApp {
    socket_path: PathBuf,

    server_task: tokio::task::JoinHandle<()>,

    history_service: Arc<ClipboardHistoryService>,

    clipboard_backend: FakeClipboardBackend,
}

impl TestIpcApp {
    async fn start() -> Self {
        Self::start_with_focus_target(Some(42)).await
    }

    async fn start_with_focus_target(target_id: Option<u64>) -> Self {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let history_service = Arc::new(ClipboardHistoryService::new(
            repository,
            HistoryConfig { max_items: 30 },
        ));

        let clipboard_backend = FakeClipboardBackend::new("");

        let clipboard_service =
            Arc::new(Mutex::new(ClipboardService::new(clipboard_backend.clone())));

        let focus_service = FocusService::new(FakeFocusBackend { target_id });

        let activation_service: Arc<TestActivationService> =
            Arc::new(ClipboardActivationService::new(
                Arc::clone(&history_service),
                clipboard_service,
                ClipboardOnlyPasteBackend,
                focus_service,
            ));

        let socket_path = temporary_socket_path();

        let server = IpcServer::bind(&socket_path).expect("IPC server bind failed");

        let service = Arc::clone(&history_service);

        let activation = Arc::clone(&activation_service);

        let server_task = tokio::spawn(async move {
            loop {
                let connection = match server.accept().await {
                    Ok(connection) => connection,

                    Err(_) => {
                        break;
                    }
                };

                let service = Arc::clone(&service);

                let activation = Arc::clone(&activation);

                tokio::spawn(async move {
                    handle_test_connection(connection, service, activation).await;
                });
            }
        });

        Self {
            socket_path,
            server_task,
            history_service,
            clipboard_backend,
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

    fn clipboard_content(&self) -> String {
        self.clipboard_backend.content()
    }

    fn socket_path(&self) -> PathBuf {
        self.socket_path.clone()
    }
}

async fn handle_test_connection<P>(
    mut connection: ipc::IpcConnection,
    history_service: Arc<ClipboardHistoryService>,
    activation_service: Arc<ClipboardActivationService<FakeClipboardBackend, P, FakeFocusBackend>>,
) where
    P: PasteBackend + Send + Sync + 'static,
{
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

        let response = handle_request(
            request,
            history_service.as_ref(),
            activation_service.as_ref(),
        )
        .await;

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

    assert_eq!(response, IpcResponse::Pong);
}

#[tokio::test]
async fn capture_focus_target_round_trips_through_daemon_ipc_stack() {
    let app = TestIpcApp::start().await;

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::CaptureFocusTarget)
        .await
        .expect("CaptureFocusTarget request failed");

    assert_eq!(
        response,
        IpcResponse::FocusTarget {
            target_id: Some(42),
        },
    );
}

#[tokio::test]
async fn unavailable_focus_target_returns_none_through_daemon_ipc_stack() {
    let app = TestIpcApp::start_with_focus_target(None).await;

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::CaptureFocusTarget)
        .await
        .expect("CaptureFocusTarget request failed");

    assert_eq!(response, IpcResponse::FocusTarget { target_id: None },);
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
async fn wayland_style_clipboard_only_activation_round_trips_through_ipc() {
    let app = TestIpcApp::start_with_focus_target(None).await;

    let service = app.history_service();

    /*
     * B starts older than A.
     *
     * We deliberately use negative
     * offsets because activation
     * promotion sets created_at to
     * Utc::now().
     */
    let b = test_item("B", "hash-b", -10);

    let b_id = b.id.to_string();

    let a = test_item("A", "hash-a", -5);

    service.save(b).await.expect("B save failed");

    service.save(a).await.expect("A save failed");

    let before = service.get_all().await.expect("history read failed");

    assert_eq!(before.len(), 2);

    assert_eq!(before[0].text_content.as_deref(), Some("A"),);

    assert_eq!(before[1].text_content.as_deref(), Some("B"),);

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::ActivateItem {
            id: b_id.clone(),
            target_id: None,
        })
        .await
        .expect("ActivateItem request failed");

    assert_eq!(
        response,
        IpcResponse::Activated {
            outcome: ActivationOutcome::ClipboardUpdated,
        },
    );

    assert_eq!(app.clipboard_content(), "B");

    let after = service
        .get_all()
        .await
        .expect("history read failed after activation");

    assert_eq!(after.len(), 2);

    assert_eq!(after[0].id, b_id);

    assert_eq!(after[0].text_content.as_deref(), Some("B"),);

    assert_eq!(after[1].text_content.as_deref(), Some("A"),);
}

#[tokio::test]
async fn activate_missing_item_returns_not_found() {
    let app = TestIpcApp::start().await;

    let mut client = app.client().await;

    let response = client
        .send(&IpcRequest::ActivateItem {
            id: "missing-item".to_string(),
            target_id: None,
        })
        .await
        .expect("ActivateItem request failed");

    assert_eq!(
        response,
        IpcResponse::Activated {
            outcome: ActivationOutcome::NotFound,
        },
    );

    assert_eq!(app.clipboard_content(), "");
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

    assert_eq!(ping, IpcResponse::Pong);

    let history = client
        .send(&IpcRequest::GetHistory)
        .await
        .expect("GetHistory failed");

    match history {
        IpcResponse::History { items } => {
            assert_eq!(items.len(), 1);
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
            assert_eq!(items.len(), 1);
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

    assert!(read_result.unwrap().is_ok(),);

    assert_eq!(
        delete_result.unwrap().expect("delete request failed"),
        IpcResponse::Deleted { deleted: true },
    );

    let items = service.get_all().await.expect("final history read failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn x11_style_direct_activation_round_trips_through_ipc() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let history_service = Arc::new(ClipboardHistoryService::new(
        repository,
        HistoryConfig { max_items: 30 },
    ));

    let item = test_item("X11 IPC item", "x11-ipc-hash", -10);

    let item_id = item.id.to_string();

    history_service
        .save(item)
        .await
        .expect("history save failed");

    let clipboard_backend = FakeClipboardBackend::new("");

    let clipboard_handle = clipboard_backend.clone();

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(clipboard_backend)));

    let pasted = Arc::new(AtomicBool::new(false));

    let paste_backend = FakeDirectPasteBackend {
        pasted: Arc::clone(&pasted),
    };

    let focus_service = FocusService::new(FakeFocusBackend {
        target_id: Some(42),
    });

    let activation_service: Arc<X11TestActivationService> =
        Arc::new(ClipboardActivationService::new(
            Arc::clone(&history_service),
            clipboard_service,
            paste_backend,
            focus_service,
        ));

    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("IPC server bind failed");

    let service = Arc::clone(&history_service);

    let activation = Arc::clone(&activation_service);

    let server_task = tokio::spawn(async move {
        let connection = server.accept().await.expect("accept failed");

        handle_test_connection(connection, service, activation).await;
    });

    let mut client = IpcClient::connect(&socket_path)
        .await
        .expect("IPC client connection failed");

    let response = client
        .send(&IpcRequest::ActivateItem {
            id: item_id.clone(),
            target_id: Some(42),
        })
        .await
        .expect("ActivateItem request failed");

    assert_eq!(
        response,
        IpcResponse::Activated {
            outcome: ActivationOutcome::Pasted,
        },
    );

    assert_eq!(clipboard_handle.content(), "X11 IPC item",);

    assert!(
        pasted.load(Ordering::SeqCst),
        "direct paste backend should have been called",
    );

    let items = history_service
        .get_all()
        .await
        .expect("history read failed");

    assert_eq!(items[0].id, item_id);

    server_task.abort();

    let _ = std::fs::remove_file(&socket_path);
}
