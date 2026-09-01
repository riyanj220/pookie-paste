use ipc::{HistoryItem, IpcClient, IpcRequest, IpcResponse};

pub async fn connect() -> Result<IpcClient, std::io::Error> {
    let path = ipc::socket_path();

    IpcClient::connect(&path).await
}

pub async fn get_history() -> Result<Vec<HistoryItem>, String> {
    let mut client = connect()
        .await
        .map_err(|error| format!("failed to connect to daemon: {error}"))?;

    let response = client
        .send(&IpcRequest::GetHistory)
        .await
        .map_err(|error| format!("failed to request history: {error:?}"))?;

    match response {
        IpcResponse::History { items } => Ok(items),

        IpcResponse::Error { message } => Err(message),

        other => Err(format!("unexpected IPC response: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn socket_path_can_be_resolved() {
        let path = ipc::socket_path();

        assert!(!path.as_os_str().is_empty());
    }
}
