use ipc::{HistoryContentRef, HistoryItem, IpcRequest, IpcResponse};

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
fn round_trips_text_history_response() {
    let item = HistoryItem::text(
        "item-1".to_string(),
        "hello".to_string(),
        "2026-08-30T10:00:00Z".to_string(),
    );

    let response = IpcResponse::History {
        items: vec![item.clone()],
    };

    let json = serde_json::to_string(&response).expect("serialization failed");

    let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(decoded, response);

    assert_eq!(item.content(), Ok(HistoryContentRef::Text("hello",),),);
}

#[test]
fn round_trips_image_history_response_as_reference_only() {
    let item = HistoryItem::image(
        "550e8400-e29b-41d4-a716-446655440000".to_string(),
        "images/550e8400-e29b-41d4-a716-446655440000.png".to_string(),
        "2026-09-17T10:00:00Z".to_string(),
    )
    .expect("image item creation failed");

    let response = IpcResponse::History {
        items: vec![item.clone()],
    };

    let json = serde_json::to_string(&response).expect("serialization failed");

    /*
     * The image itself must never enter the IPC frame.
     */
    assert!(json.contains(r#""content_type":"image""#,));

    assert!(json.contains("images/550e8400-e29b-41d4-a716-446655440000.png",));

    assert!(!json.contains("image_bytes",));

    assert!(!json.contains("base64",));

    let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(decoded, response);

    assert_eq!(
        item.content(),
        Ok(HistoryContentRef::Image {
            file_path: "images/550e8400-e29b-41d4-a716-446655440000.png",
        },),
    );
}

#[test]
fn image_history_metadata_frame_is_small() {
    let item = HistoryItem::image(
        "550e8400-e29b-41d4-a716-446655440000".to_string(),
        "images/550e8400-e29b-41d4-a716-446655440000.png".to_string(),
        "2026-09-17T10:00:00Z".to_string(),
    )
    .expect("image item creation failed");

    let response = IpcResponse::History { items: vec![item] };

    let frame = ipc::encode(&response).expect("IPC encoding failed");

    /*
     * A metadata-only image item should remain tiny compared
     * with the 1 MiB IPC frame limit regardless of the real
     * PNG's size on disk.
     */
    assert!(
        frame.len() < 1024,
        "image metadata IPC frame unexpectedly large: {} bytes",
        frame.len(),
    );
}

#[test]
fn serializes_toggle_pin_request() {
    let request = IpcRequest::TogglePinItem {
        id: "item-456".to_string(),
    };

    let json = serde_json::to_string(&request).expect("serialization failed");

    assert_eq!(json, r#"{"type":"toggle_pin_item","id":"item-456"}"#);

    let decoded: IpcRequest = serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(decoded, request);
}

#[test]
fn round_trips_pin_toggled_response() {
    let response = IpcResponse::PinToggled {
        id: "item-456".to_string(),
        is_pinned: true,
    };

    let json = serde_json::to_string(&response).expect("serialization failed");

    assert_eq!(
        json,
        r#"{"type":"pin_toggled","id":"item-456","is_pinned":true}"#
    );

    let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(decoded, response);
}

#[test]
fn history_item_deserializes_without_pinned_at_backward_compatible() {
    let legacy_json = r#"{
        "id": "item-legacy",
        "content_type": "text",
        "text_content": "legacy text",
        "file_path": null,
        "created_at": "2026-09-17T10:00:00Z"
    }"#;

    let item: HistoryItem = serde_json::from_str(legacy_json).expect("deserialization failed");
    assert_eq!(item.pinned_at, None);
    assert!(!item.is_pinned());
}

#[test]
fn history_item_round_trips_with_pinned_at() {
    let item = HistoryItem::text(
        "item-pinned".to_string(),
        "pinned content".to_string(),
        "2026-09-17T10:00:00Z".to_string(),
    )
    .with_pinned_at(Some("2026-09-20T12:00:00Z".to_string()));

    assert!(item.is_pinned());

    let json = serde_json::to_string(&item).expect("serialization failed");
    let decoded: HistoryItem = serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(decoded, item);
    assert_eq!(decoded.pinned_at.as_deref(), Some("2026-09-20T12:00:00Z"));
}
