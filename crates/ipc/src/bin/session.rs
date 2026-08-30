use ipc::{IpcClient, IpcRequest, IpcResponse, socket_path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = socket_path();

    let mut client = IpcClient::connect(&path)
        .await
        .map_err(|error| format!("failed to connect to {}: {error}", path.display()))?;

    let first = client
        .send(&IpcRequest::Ping)
        .await
        .map_err(|error| format!("first request failed: {error:?}"))?;

    println!("First response: {:?}", first);

    let second = client
        .send(&IpcRequest::GetHistory)
        .await
        .map_err(|error| format!("second request failed: {error:?}"))?;

    match second {
        IpcResponse::History { items } => {
            println!("History items: {}", items.len());
        }

        other => {
            println!("Second response: {:?}", other);
        }
    }

    let third = client
        .send(&IpcRequest::Ping)
        .await
        .map_err(|error| format!("third request failed: {error:?}"))?;

    println!("Third response: {:?}", third);

    Ok(())
}
