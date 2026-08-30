use history::ClipboardHistoryService;
use ipc::{IpcServer, socket_path};
use tracing::{error, info};

use crate::request_handler::handle_request;

pub async fn run(history_service: &ClipboardHistoryService<'_>) -> anyhow::Result<()> {
    let path = socket_path();

    let server = IpcServer::bind(&path)
        .map_err(|error| anyhow::anyhow!("failed to bind IPC server: {error:?}"))?;

    info!("IPC server listening at {}", path.display());

    loop {
        let mut connection = match server.accept().await {
            Ok(connection) => connection,

            Err(error) => {
                error!("failed to accept IPC connection: {:?}", error);

                continue;
            }
        };

        match connection.read_request().await {
            Ok(request) => {
                let response = handle_request(request, history_service).await;

                if let Err(error) = connection.send_response(&response).await {
                    error!("failed to send IPC response: {:?}", error);
                }
            }

            Err(error) => {
                error!("failed to read IPC request: {:?}", error);
            }
        }
    }
}
