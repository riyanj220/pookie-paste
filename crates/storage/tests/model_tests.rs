use storage::StoredClipboardItem;

#[test]
fn creates_storage_model() {
    let item = StoredClipboardItem {
        id: "123".to_string(),

        content_type: "text".to_string(),

        text_content: Some("Hello".to_string()),

        file_path: None,

        content_hash: "abc".to_string(),

        created_at: "2026-01-01".to_string(),

        pinned_at: None,
    };

    assert_eq!(item.text_content.unwrap(), "Hello");
    assert_eq!(item.pinned_at, None);
}
