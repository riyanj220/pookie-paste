use ipc::{IpcClient, IpcRequest, IpcResponse, socket_path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id = std::env::args().nth(1).ok_or("usage: pin <item-id>")?;

    let path = socket_path();

    let mut client = IpcClient::connect(&path)
        .await
        .map_err(|error| format!("failed to connect to {}: {error}", path.display()))?;

    let response = client
        .send(&IpcRequest::TogglePinItem { id })
        .await
        .map_err(|error| format!("IPC request failed: {error:?}"))?;

    match response {
        IpcResponse::PinToggled { id, is_pinned } => {
            println!("Item {}: pinned={}", id, is_pinned);
        }

        IpcResponse::Error { message } => {
            return Err(message.into());
        }

        other => {
            return Err(format!("unexpected response: {other:?}").into());
        }
    }

    Ok(())
}
