use std::sync::{Arc, RwLock};
use std::time::Duration;

use daemon::activation_service::ClipboardActivationService;
use daemon::focus_backend::FocusBackend;
use daemon::paste_backend::PasteBackend;
use daemon::reload_coordinator::ReloadCoordinator;
use daemon::request_handler::handle_request;
use daemon::ui_launcher::UiLauncher;

use history::ClipboardHistoryService;

use ipc::{IpcConnection, IpcServer, ServerError, ShortcutStatusInfo, socket_path};

use pookie_clipboard::ClipboardBackend;

use tokio::time::timeout;

use tracing::{error, info};

const IPC_READ_TIMEOUT: Duration = Duration::from_secs(30);

pub enum BindOutcome {
    Bound(IpcServer),
    AlreadyRunning,
}

pub fn bind() -> anyhow::Result<BindOutcome> {
    let path = socket_path();

    match IpcServer::bind(&path) {
        Ok(server) => {
            info!("IPC server listening at {}", path.display());

            Ok(BindOutcome::Bound(server))
        }

        Err(ServerError::AlreadyRunning { path }) => {
            info!(
                path = %path.display(),
                  "Pookie Paste is already running"
            );

            Ok(BindOutcome::AlreadyRunning)
        }

        Err(error) => Err(anyhow::anyhow!("failed to bind IPC server: {error:?}")),
    }
}

pub async fn run<B, P, F>(
    server: IpcServer,
    history_service: Arc<ClipboardHistoryService>,
    activation_service: Arc<ClipboardActivationService<B, P, F>>,
    ui_launcher: Arc<UiLauncher>,
    shortcut_status: Arc<RwLock<ShortcutStatusInfo>>,
    reload_coordinator: Arc<ReloadCoordinator>,
) -> anyhow::Result<()>
where
    B: ClipboardBackend + Send + Sync + 'static,
    P: PasteBackend + Send + Sync + 'static,
    F: FocusBackend + Send + Sync + 'static,
{
    loop {
        let connection = match server.accept().await {
            Ok(connection) => connection,

            Err(error) => {
                error!("failed to accept IPC connection: {:?}", error);

                continue;
            }
        };

        let history_service = Arc::clone(&history_service);

        let activation_service = Arc::clone(&activation_service);

        let ui_launcher = Arc::clone(&ui_launcher);

        let shortcut_status = Arc::clone(&shortcut_status);

        let reload_coordinator = Arc::clone(&reload_coordinator);

        tokio::spawn(async move {
            handle_connection(
                connection,
                history_service,
                activation_service,
                ui_launcher,
                shortcut_status,
                reload_coordinator,
            )
            .await;
        });
    }
}

async fn handle_connection<B, P, F>(
    connection: IpcConnection,
    history_service: Arc<ClipboardHistoryService>,
    activation_service: Arc<ClipboardActivationService<B, P, F>>,
    ui_launcher: Arc<UiLauncher>,
    shortcut_status: Arc<RwLock<ShortcutStatusInfo>>,
    reload_coordinator: Arc<ReloadCoordinator>,
) where
    B: ClipboardBackend + Send + Sync + 'static,
    P: PasteBackend + Send + Sync + 'static,
    F: FocusBackend + Send + Sync + 'static,
{
    handle_connection_with_timeout(
        connection,
        history_service,
        activation_service,
        ui_launcher,
        shortcut_status,
        reload_coordinator,
        IPC_READ_TIMEOUT,
    )
    .await;
}

async fn handle_connection_with_timeout<B, P, F>(
    mut connection: IpcConnection,
    history_service: Arc<ClipboardHistoryService>,
    activation_service: Arc<ClipboardActivationService<B, P, F>>,
    ui_launcher: Arc<UiLauncher>,
    shortcut_status: Arc<RwLock<ShortcutStatusInfo>>,
    reload_coordinator: Arc<ReloadCoordinator>,
    read_timeout: Duration,
) where
    B: ClipboardBackend + Send + Sync + 'static,
    P: PasteBackend + Send + Sync + 'static,
    F: FocusBackend + Send + Sync + 'static,
{
    loop {
        let request = match timeout(read_timeout, connection.read_request()).await {
            Ok(Ok(request)) => request,

            Ok(Err(ipc::ServerError::ConnectionClosed)) => {
                break;
            }

            Ok(Err(error)) => {
                error!("failed to read IPC request: {:?}", error);

                break;
            }

            Err(_) => {
                info!("closing idle IPC connection after {:?}", read_timeout);

                break;
            }
        };

        let response = handle_request(
            request,
            history_service.as_ref(),
            activation_service.as_ref(),
            ui_launcher.as_ref(),
            &shortcut_status,
            &reload_coordinator,
        )
        .await;

        if let Err(error) = connection.send_response(&response).await {
            error!("failed to send IPC response: {:?}", error);

            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};

    use daemon::clipboard_service::ClipboardService;
    use daemon::clipboard_state::ClipboardState;
    use daemon::focus_backend::{FocusBackend, FocusError, FocusTarget};
    use daemon::focus_service::FocusService;
    use daemon::paste_backend::WaylandPasteBackend;

    use history::{ClipboardHistoryService, HistoryConfig};

    use pookie_clipboard::{ClipboardBackend, ClipboardError};

    use storage::{Database, StorageRepository};

    use tokio::sync::Mutex;

    static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temporary_socket_path() -> PathBuf {
        let counter = SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "pookie-paste-daemon-ipc-test-{}-{}.sock",
            std::process::id(),
            counter,
        ))
    }

    #[derive(Clone)]
    struct FakeClipboardBackend {
        content: Arc<StdMutex<String>>,
    }

    impl FakeClipboardBackend {
        fn new() -> Self {
            Self {
                content: Arc::new(StdMutex::new(String::new())),
            }
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
     * IPC tests are not testing real window focus.
     *
     * This fake immediately accepts restoration and
     * reports the requested target as active.
     */
    struct ImmediateFocusBackend;

    impl FocusBackend for ImmediateFocusBackend {
        fn active_target(&self) -> Result<FocusTarget, FocusError> {
            Ok(FocusTarget::new(1))
        }

        fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
            Ok(())
        }

        fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn closes_idle_connection_after_timeout() {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let history_service = Arc::new(ClipboardHistoryService::new(
            repository,
            HistoryConfig { max_items: 30 },
        ));

        let clipboard_backend = FakeClipboardBackend::new();

        let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(
            clipboard_backend,
            Arc::new(ClipboardState::default()),
        )));

        let focus_service = FocusService::new(ImmediateFocusBackend);

        let activation_service = Arc::new(ClipboardActivationService::new(
            Arc::clone(&history_service),
            clipboard_service,
            WaylandPasteBackend::new(),
            focus_service,
        ));

        let socket_path = temporary_socket_path();

        let server = IpcServer::bind(&socket_path).expect("server bind failed");

        let service = Arc::clone(&history_service);

        let activation = Arc::clone(&activation_service);

        let connection_task = tokio::spawn(async move {
            let connection = server.accept().await.expect("accept failed");
            let ui_launcher = Arc::new(UiLauncher::new());
            let shortcut_status = Arc::new(RwLock::new(ShortcutStatusInfo {
                configured_shortcut: "Super+V".to_string(),
                backend_name: None,
                capability: None,
                effective_shortcut: None,
                state: ipc::IpcShortcutState::Initializing,
            }));
            let reload_handle = daemon::shortcut_listener::ShortcutReloadHandle::new_test_handle(
                Arc::clone(&shortcut_status),
            );
            let dummy = std::path::PathBuf::from("/nonexistent/test/pookie/config.toml");
            let reload_coordinator = Arc::new(ReloadCoordinator::with_custom_paths(
                reload_handle,
                dummy.clone(),
                dummy,
            ));

            handle_connection_with_timeout(
                connection,
                service,
                activation,
                ui_launcher,
                shortcut_status,
                reload_coordinator,
                Duration::from_millis(50),
            )
            .await;
        });

        let _idle_client = tokio::net::UnixStream::connect(&socket_path)
            .await
            .expect("idle client connection failed");

        let result = tokio::time::timeout(Duration::from_secs(1), connection_task).await;

        assert!(
            result.is_ok(),
            "idle connection task did not finish after timeout"
        );

        result.unwrap().expect("connection task panicked");
    }
}
