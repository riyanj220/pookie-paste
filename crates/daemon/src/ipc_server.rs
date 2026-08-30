use std::sync::Arc;

use history::ClipboardHistoryService;
use ipc::{IpcConnection, IpcServer, socket_path};
use tracing::{error, info};

use crate::request_handler::handle_request;

pub async fn run(history_service: Arc<ClipboardHistoryService>) -> anyhow::Result<()> {
    let path = socket_path();

    let server = IpcServer::bind(&path)
        .map_err(|error| anyhow::anyhow!("failed to bind IPC server: {error:?}"))?;

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
    mut connection: IpcConnection,
    history_service: Arc<ClipboardHistoryService>,
) {
    loop {
        let request = match connection.read_request().await {
            Ok(request) => request,

            Err(ipc::ServerError::ConnectionClosed) => {
                break;
            }

            Err(error) => {
                error!("failed to read IPC request: {:?}", error);

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
