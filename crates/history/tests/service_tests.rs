use chrono::Utc;
use history::{ClipboardHistoryService, HistoryConfig};
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

#[tokio::test]
async fn retrieves_saved_history() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let first = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("First item".to_string()),
        hash: "first-hash".to_string(),
        created_at: Utc::now(),
    };

    let second = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("Second item".to_string()),
        hash: "second-hash".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(1),
    };

    service.save(first).await.expect("first save failed");

    service.save(second).await.expect("second save failed");

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 2);

    assert_eq!(items[0].text_content.as_deref(), Some("Second item"),);

    assert_eq!(items[1].text_content.as_deref(), Some("First item"),);
}

#[tokio::test]
async fn deletes_history_item() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let item_id = uuid::Uuid::new_v4();

    let item = ClipboardItem {
        id: item_id,
        content: ClipboardContent::Text("Delete me".to_string()),
        hash: "delete-hash".to_string(),
        created_at: Utc::now(),
    };

    service.save(item).await.expect("save failed");

    let deleted = service
        .delete(&item_id.to_string())
        .await
        .expect("delete failed");

    assert!(deleted);

    let items = service.get_all().await.expect("history retrieval failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn returns_false_when_deleting_missing_item() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let deleted = service
        .delete("does-not-exist")
        .await
        .expect("delete failed");

    assert!(!deleted);
}

#[tokio::test]
async fn clears_history() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    for index in 1..=3 {
        let item = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text(format!("Item {index}")),
            hash: format!("hash-{index}"),
            created_at: Utc::now() + chrono::Duration::seconds(index),
        };

        service.save(item).await.expect("save failed");
    }

    let deleted_count = service.clear().await.expect("clear failed");

    assert_eq!(deleted_count, 3);

    let items = service.get_all().await.expect("history retrieval failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn clearing_empty_history_returns_zero() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let deleted_count = service.clear().await.expect("clear failed");

    assert_eq!(deleted_count, 0);
}

#[tokio::test]
async fn supports_complete_history_lifecycle() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let first_id = uuid::Uuid::new_v4();
    let second_id = uuid::Uuid::new_v4();

    let first = ClipboardItem {
        id: first_id,
        content: ClipboardContent::Text("First item".to_string()),
        hash: "first-hash".to_string(),
        created_at: Utc::now(),
    };

    let second = ClipboardItem {
        id: second_id,
        content: ClipboardContent::Text("Second item".to_string()),
        hash: "second-hash".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(1),
    };

    service.save(first).await.expect("first save failed");

    service.save(second).await.expect("second save failed");

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 2);

    assert_eq!(items[0].text_content.as_deref(), Some("Second item"));

    let deleted = service
        .delete(&first_id.to_string())
        .await
        .expect("delete failed");

    assert!(deleted);

    let deleted_again = service
        .delete(&first_id.to_string())
        .await
        .expect("second delete failed");

    assert!(!deleted_again);

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 1);

    assert_eq!(items[0].text_content.as_deref(), Some("Second item"));

    let cleared = service.clear().await.expect("clear failed");

    assert_eq!(cleared, 1);

    let cleared_again = service.clear().await.expect("second clear failed");

    assert_eq!(cleared_again, 0);

    let items = service.get_all().await.expect("history retrieval failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn moves_repeated_content_to_most_recent() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let base_time = Utc::now();

    let first_a = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("A".to_string()),
        hash: "hash-a".to_string(),
        created_at: base_time,
    };

    let b = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("B".to_string()),
        hash: "hash-b".to_string(),
        created_at: base_time + chrono::Duration::seconds(1),
    };

    let second_a = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("A".to_string()),
        hash: "hash-a".to_string(),
        created_at: base_time + chrono::Duration::seconds(2),
    };

    service.save(first_a).await.unwrap();
    service.save(b).await.unwrap();
    service.save(second_a).await.unwrap();

    let items = service.get_all().await.unwrap();

    assert_eq!(items.len(), 2);

    assert_eq!(items[0].text_content.as_deref(), Some("A"),);

    assert_eq!(items[1].text_content.as_deref(), Some("B"),);
}
