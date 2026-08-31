use ipc::{IpcClient, IpcRequest};

#[tokio::main]
async fn main() {
    let id = std::env::args()
        .nth(1)
        .expect("usage: activate <item-id> [target-id]");

    let target_id = std::env::args()
        .nth(2)
        .map(|value| value.parse::<u64>().expect("target ID must be an integer"));

    let path = ipc::socket_path();

    let mut client = IpcClient::connect(&path)
        .await
        .expect("failed to connect to daemon");

    let response = client
        .send(&IpcRequest::ActivateItem { id, target_id })
        .await
        .expect("activation request failed");

    println!("{response:?}");
}
