use storage::{Database, StorageRepository, StoredClipboardItem};

#[tokio::test]
async fn inserts_and_reads_clipboard_item() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let item = StoredClipboardItem {
        id: "123".to_string(),

        content_type: "text".to_string(),

        text_content: Some("Hello Pookie".to_string()),

        file_path: None,

        content_hash: "hash123".to_string(),

        created_at: "2026-08-29T00:00:00Z".to_string(),
    };

    repository.insert(&item).await.expect("insert failed");

    let items = repository.get_all().await.expect("query failed");

    assert_eq!(items.len(), 1);

    assert_eq!(items[0].text_content.as_deref(), Some("Hello Pookie"));
}
