use ipc::{HistoryItem, IpcRequest, IpcResponse};

#[test]
fn serializes_ping_request() {
    let request = IpcRequest::Ping;

    let json = serde_json::to_string(&request).expect("serialization failed");

    assert_eq!(json, r#"{"type":"ping"}"#,);
}

#[test]
fn round_trips_delete_request() {
    let request = IpcRequest::DeleteItem {
        id: "item-123".to_string(),
    };

    let json = serde_json::to_string(&request).expect("serialization failed");

    let decoded: IpcRequest = serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(decoded, request);
}

#[test]
fn round_trips_history_response() {
    let response = IpcResponse::History {
        items: vec![HistoryItem {
            id: "item-1".to_string(),
            content_type: "text".to_string(),
            text_content: Some("hello".to_string()),
            file_path: None,
            created_at: "2026-08-30T10:00:00Z".to_string(),
        }],
    };

    let json = serde_json::to_string(&response).expect("serialization failed");

    let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(decoded, response);
}
