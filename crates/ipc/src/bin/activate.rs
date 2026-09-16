use ipc::{IpcClient, IpcFocusTarget, IpcRequest};

fn parse_target(value: &str) -> IpcFocusTarget {
    if let Some(value) = value.strip_prefix("x11:") {
        let id = value.parse::<u64>().expect("X11 target must be an integer");

        return IpcFocusTarget::X11(id);
    }

    if let Some(value) = value.strip_prefix("kde:") {
        if value.is_empty() {
            panic!("KDE target UUID cannot be empty");
        }

        return IpcFocusTarget::Kde(value.to_string());
    }

    /*
     * Backwards-friendly CLI behavior:
     *
     * A bare integer still means X11.
     */
    let id = value
        .parse::<u64>()
        .expect("target must be x11:<id>, kde:<uuid>, or a bare X11 integer");

    IpcFocusTarget::X11(id)
}

#[tokio::main]
async fn main() {
    let id = std::env::args()
        .nth(1)
        .expect("usage: activate <item-id> [x11:<window-id>|kde:<uuid>]");

    let target_id = std::env::args().nth(2).map(|value| parse_target(&value));

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
