use storage::StoredClipboardItem;

#[test]
fn creates_storage_model() {
    let item = StoredClipboardItem {
        id: "123".to_string(),

        content: "Hello".to_string(),

        content_hash: "abc".to_string(),

        content_type: "text".to_string(),

        created_at: "2026-01-01".to_string(),
    };

    assert_eq!(item.content, "Hello");
}
