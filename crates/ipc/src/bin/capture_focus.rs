use ipc::{IpcClient, IpcRequest};

#[tokio::main]
async fn main() {
    let path = ipc::socket_path();

    let mut client = IpcClient::connect(&path)
        .await
        .expect("failed to connect to daemon");

    let response = client
        .send(&IpcRequest::CaptureFocusTarget)
        .await
        .expect("focus capture request failed");

    println!("{response:?}");
}
