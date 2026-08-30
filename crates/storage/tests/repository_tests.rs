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

async fn create_repository() -> StorageRepository {
    let database = Box::new(
        Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed"),
    );

    let database = Box::leak(database);

    StorageRepository::new(database)
}

#[tokio::test]
async fn counts_items() {
    let repository = create_repository().await;

    let count = repository.count().await.unwrap();

    assert_eq!(count, 0);
}

#[tokio::test]
async fn deletes_items() {
    let repository = create_repository().await;

    let item = StoredClipboardItem {
        id: "delete-test".to_string(),

        content_type: "text".to_string(),

        text_content: Some("Delete me".to_string()),

        file_path: None,

        content_hash: "hash".to_string(),

        created_at: "2026-08-29".to_string(),
    };

    repository.insert(&item).await.unwrap();

    repository
        .delete_by_ids(vec!["delete-test".to_string()])
        .await
        .unwrap();

    let count = repository.count().await.unwrap();

    assert_eq!(count, 0);
}

#[tokio::test]
async fn deletes_single_item_by_id() {
    let repository = create_repository().await;

    let item_a = StoredClipboardItem {
        id: "item-a".to_string(),
        content_type: "text".to_string(),
        text_content: Some("Item A".to_string()),
        file_path: None,
        content_hash: "hash-a".to_string(),
        created_at: "2026-08-29T00:00:01Z".to_string(),
    };

    let item_b = StoredClipboardItem {
        id: "item-b".to_string(),
        content_type: "text".to_string(),
        text_content: Some("Item B".to_string()),
        file_path: None,
        content_hash: "hash-b".to_string(),
        created_at: "2026-08-29T00:00:02Z".to_string(),
    };

    repository.insert(&item_a).await.unwrap();
    repository.insert(&item_b).await.unwrap();

    let deleted = repository.delete_by_id("item-a").await.unwrap();

    assert!(deleted);

    let deleted_again = repository.delete_by_id("item-a").await.unwrap();

    assert!(!deleted_again);

    let items = repository.get_all().await.unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "item-b");
}

#[tokio::test]
async fn clears_all_items() {
    let repository = create_repository().await;

    for index in 1..=3 {
        let item = StoredClipboardItem {
            id: format!("item-{index}"),
            content_type: "text".to_string(),
            text_content: Some(format!("Item {index}")),
            file_path: None,
            content_hash: format!("hash-{index}"),
            created_at: format!("2026-08-29T00:00:0{index}Z"),
        };

        repository.insert(&item).await.unwrap();
    }

    let deleted_count = repository.clear().await.unwrap();

    assert_eq!(deleted_count, 3);

    let count = repository.count().await.unwrap();

    assert_eq!(count, 0);
}

#[tokio::test]
async fn finds_item_by_existing_hash() {
    let repository = create_repository().await;

    let item = StoredClipboardItem {
        id: "item-a".to_string(),
        content_type: "text".to_string(),
        text_content: Some("Item A".to_string()),
        file_path: None,
        content_hash: "hash-a".to_string(),
        created_at: "2026-08-29T00:00:01Z".to_string(),
    };

    repository.insert(&item).await.unwrap();

    let found = repository.find_by_hash("hash-a").await.unwrap();

    assert!(found.is_some());

    let found = found.unwrap();

    assert_eq!(found.id, "item-a");
    assert_eq!(found.content_hash, "hash-a");
    assert_eq!(found.text_content.as_deref(), Some("Item A"));
}

#[tokio::test]
async fn returns_none_for_missing_hash() {
    let repository = create_repository().await;

    let found = repository.find_by_hash("missing-hash").await.unwrap();

    assert!(found.is_none());
}

#[tokio::test]
async fn repository_owns_database_pool_handle() {
    let repository = {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        StorageRepository::new(&database)
    };

    let count = repository.count().await.expect("count failed");

    assert_eq!(count, 0);
}
