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

pub async fn capture_focus_target() -> Result<Option<u64>, String> {
    let mut client = connect()
        .await
        .map_err(|error| format!("failed to connect to daemon: {error}"))?;

    let response = client
        .send(&ipc::IpcRequest::CaptureFocusTarget)
        .await
        .map_err(|error| format!("failed to capture focus target: {error:?}"))?;

    match response {
        ipc::IpcResponse::FocusTarget { target_id } => Ok(target_id),

        ipc::IpcResponse::Error { message } => Err(message),

        other => Err(format!("unexpected IPC response: {other:?}")),
    }
}

pub async fn activate_item(
    id: String,
    target_id: Option<u64>,
) -> Result<ipc::ActivationOutcome, String> {
    let mut client = connect()
        .await
        .map_err(|error| format!("failed to connect to daemon: {error}"))?;

    let response = client
        .send(&ipc::IpcRequest::ActivateItem { id, target_id })
        .await
        .map_err(|error| format!("failed to activate item: {error:?}"))?;

    match response {
        ipc::IpcResponse::Activated { outcome } => Ok(outcome),

        ipc::IpcResponse::Error { message } => Err(message),

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
