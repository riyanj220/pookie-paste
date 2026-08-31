use std::sync::Arc;
use std::time::Duration;

use daemon::request_handler::handle_request;
use history::ClipboardHistoryService;
use ipc::{IpcConnection, IpcServer, socket_path};
use tokio::time::timeout;
use tracing::{error, info};

const IPC_READ_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn run(history_service: Arc<ClipboardHistoryService>) -> anyhow::Result<()> {
    let path = socket_path();

    let server = IpcServer::bind(&path).map_err(|error| match error {
        ipc::ServerError::AlreadyRunning { path } => {
            anyhow::anyhow!(
                "another Pookie Paste daemon is already running at {}",
                path.display()
            )
        }

        other => {
            anyhow::anyhow!("failed to bind IPC server: {other:?}")
        }
    })?;

    info!("IPC server listening at {}", path.display());

    loop {
        let connection = match server.accept().await {
            Ok(connection) => connection,

            Err(error) => {
                error!("failed to accept IPC connection: {:?}", error);

                continue;
            }
        };

        let history_service = Arc::clone(&history_service);

        tokio::spawn(async move {
            handle_connection(connection, history_service).await;
        });
    }
}

async fn handle_connection(
    connection: IpcConnection,
    history_service: Arc<ClipboardHistoryService>,
) {
    handle_connection_with_timeout(connection, history_service, IPC_READ_TIMEOUT).await;
}

async fn handle_connection_with_timeout(
    mut connection: IpcConnection,
    history_service: Arc<ClipboardHistoryService>,
    read_timeout: Duration,
) {
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

        let response = handle_request(request, history_service.as_ref()).await;

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

    use history::{ClipboardHistoryService, HistoryConfig};
    use storage::{Database, StorageRepository};

    static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temporary_socket_path() -> PathBuf {
        let counter = SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "pookie-paste-daemon-ipc-test-{}-{}.sock",
            std::process::id(),
            counter,
        ))
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

        let socket_path = temporary_socket_path();

        let server = IpcServer::bind(&socket_path).expect("server bind failed");

        let service = Arc::clone(&history_service);

        let connection_task = tokio::spawn(async move {
            let connection = server.accept().await.expect("accept failed");

            handle_connection_with_timeout(connection, service, Duration::from_millis(50)).await;
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
