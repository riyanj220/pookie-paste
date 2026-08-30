use ipc::{IpcRequest, IpcResponse, IpcServer, socket_path};
use tracing::{error, info};

pub async fn run() -> anyhow::Result<()> {
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
            Ok(IpcRequest::Ping) => {
                if let Err(error) = connection.send_response(&IpcResponse::Pong).await {
                    error!("failed to send IPC response: {:?}", error);
                }
            }

            Ok(request) => {
                error!("unsupported IPC request: {:?}", request);

                if let Err(error) = connection
                    .send_response(&IpcResponse::Error {
                        message: "request not implemented".to_string(),
                    })
                    .await
                {
                    error!("failed to send IPC error response: {:?}", error);
                }
            }

            Err(error) => {
                error!("failed to read IPC request: {:?}", error);
            }
        }
    }
}
