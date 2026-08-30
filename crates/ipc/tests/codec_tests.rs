use ipc::{IpcRequest, decode, encode};

#[test]
fn encodes_request_with_newline() {
    let bytes = encode(&IpcRequest::Ping).expect("encoding failed");

    assert_eq!(bytes, b"{\"type\":\"ping\"}\n",);
}

#[test]
fn round_trips_request_frame() {
    let request = IpcRequest::DeleteItem {
        id: "item-123".to_string(),
    };

    let bytes = encode(&request).expect("encoding failed");

    let decoded: IpcRequest = decode(&bytes).expect("decoding failed");

    assert_eq!(decoded, request,);
}

#[test]
fn rejects_frame_without_newline() {
    let frame = b"{\"type\":\"ping\"}";

    let result = decode::<IpcRequest>(frame);

    assert!(result.is_err());
}
