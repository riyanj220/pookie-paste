use chrono::Utc;
use history::{ClipboardHistoryService, config::HistoryConfig};
use pookie_clipboard::ClipboardContent;
use pookie_core::ClipboardItem;
use storage::{Database, StorageRepository, StoredClipboardItem};

#[tokio::test]
async fn enforces_history_limit() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 3 });

    for index in 1..=4 {
        let content = ClipboardContent::Text(format!("Item {index}"));

        let item = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content,
            hash: format!("hash-{index}"),
            created_at: Utc::now() + chrono::Duration::seconds(index),
        };

        service.save(item).await.expect("save failed");
    }

    let repository = StorageRepository::new(&database);

    let items = repository.get_all().await.expect("query failed");

    assert_eq!(items.len(), 3);

    assert!(
        items
            .iter()
            .all(|item| item.text_content.as_deref() != Some("Item 1"))
    );
}

#[tokio::test]
async fn keeps_items_when_below_history_limit() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 3 });

    for index in 1..=2 {
        let item = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text(format!("Item {index}")),
            hash: format!("hash-{index}"),
            created_at: Utc::now() + chrono::Duration::seconds(index),
        };

        service.save(item).await.expect("save failed");
    }

    let repository = StorageRepository::new(&database);

    let items = repository.get_all().await.expect("query failed");

    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn keeps_items_when_exactly_at_history_limit() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 3 });

    for index in 1..=3 {
        let item = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text(format!("Item {index}")),
            hash: format!("hash-{index}"),
            created_at: Utc::now() + chrono::Duration::seconds(index),
        };

        service.save(item).await.expect("save failed");
    }

    let repository = StorageRepository::new(&database);

    let items = repository.get_all().await.expect("query failed");

    assert_eq!(items.len(), 3);
}

#[tokio::test]
async fn removes_multiple_excess_items() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let base_time = Utc::now();

    for index in 1..=5 {
        let item = StoredClipboardItem {
            id: format!("direct-{index}"),
            content_type: "text".to_string(),
            text_content: Some(format!("Item {index}")),
            file_path: None,
            content_hash: format!("hash-{index}"),
            created_at: (base_time + chrono::Duration::seconds(index)).to_rfc3339(),
        };

        repository.insert(&item).await.expect("insert failed");
    }

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 3 });

    let item = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("Item 6".to_string()),
        hash: "hash-6".to_string(),
        created_at: base_time + chrono::Duration::seconds(6),
    };

    service.save(item).await.expect("save failed");

    let repository = StorageRepository::new(&database);

    let items = repository.get_all().await.expect("query failed");

    assert_eq!(items.len(), 3);

    assert!(items.iter().all(|item| {
        matches!(
            item.text_content.as_deref(),
            Some("Item 4") | Some("Item 5") | Some("Item 6")
        )
    }));
}
