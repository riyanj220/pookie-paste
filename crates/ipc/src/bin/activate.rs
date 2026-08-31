use ipc::{IpcClient, IpcRequest};

#[tokio::main]
async fn main() {
    let id = std::env::args().nth(1).expect("usage: activate <item-id>");

    let path = ipc::socket_path();

    let mut client = IpcClient::connect(&path)
        .await
        .expect("failed to connect to daemon");

    let response = client
        .send(&IpcRequest::ActivateItem { id })
        .await
        .expect("activation request failed");

    println!("{response:?}");
}
