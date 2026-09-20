use std::sync::Arc;

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
            .all(|item| { item.text_content.as_deref() != Some("Item 1") })
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
            pinned_at: None,
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

    assert_eq!(items[0].text_content.as_deref(), Some("Second item"),);

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

    assert_eq!(items[0].text_content.as_deref(), Some("Second item"),);

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

#[tokio::test]
async fn supports_concurrent_history_reads() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = Arc::new(ClipboardHistoryService::new(
        repository,
        HistoryConfig { max_items: 30 },
    ));

    let first_item = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("First".to_string()),
        hash: "concurrent-read-first".to_string(),
        created_at: Utc::now(),
    };

    let second_item = ClipboardItem {
        id: uuid::Uuid::new_v4(),
        content: ClipboardContent::Text("Second".to_string()),
        hash: "concurrent-read-second".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(1),
    };

    service.save(first_item).await.expect("first save failed");

    service.save(second_item).await.expect("second save failed");

    let first_service = Arc::clone(&service);

    let second_service = Arc::clone(&service);

    let third_service = Arc::clone(&service);

    let first =
        tokio::spawn(async move { first_service.get_all().await.expect("first read failed") });

    let second =
        tokio::spawn(async move { second_service.get_all().await.expect("second read failed") });

    let third =
        tokio::spawn(async move { third_service.get_all().await.expect("third read failed") });

    let first = first.await.expect("first task failed");

    let second = second.await.expect("second task failed");

    let third = third.await.expect("third task failed");

    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);
    assert_eq!(third.len(), 2);
}

#[tokio::test]
async fn supports_concurrent_history_deletes() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = Arc::new(ClipboardHistoryService::new(
        repository,
        HistoryConfig { max_items: 30 },
    ));

    let a_id = uuid::Uuid::new_v4();

    let b_id = uuid::Uuid::new_v4();

    let c_id = uuid::Uuid::new_v4();

    let a = ClipboardItem {
        id: a_id,
        content: ClipboardContent::Text("A".to_string()),
        hash: "concurrent-delete-a".to_string(),
        created_at: Utc::now(),
    };

    let b = ClipboardItem {
        id: b_id,
        content: ClipboardContent::Text("B".to_string()),
        hash: "concurrent-delete-b".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(1),
    };

    let c = ClipboardItem {
        id: c_id,
        content: ClipboardContent::Text("C".to_string()),
        hash: "concurrent-delete-c".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(2),
    };

    service.save(a).await.expect("save A failed");

    service.save(b).await.expect("save B failed");

    service.save(c).await.expect("save C failed");

    let a_id = a_id.to_string();

    let b_id = b_id.to_string();

    let c_id = c_id.to_string();

    let delete_a = {
        let service = Arc::clone(&service);

        tokio::spawn(async move { service.delete(&a_id).await.expect("delete A failed") })
    };

    let delete_b = {
        let service = Arc::clone(&service);

        tokio::spawn(async move { service.delete(&b_id).await.expect("delete B failed") })
    };

    let delete_c = {
        let service = Arc::clone(&service);

        tokio::spawn(async move { service.delete(&c_id).await.expect("delete C failed") })
    };

    assert!(delete_a.await.expect("delete A task failed"));

    assert!(delete_b.await.expect("delete B task failed"));

    assert!(delete_c.await.expect("delete C task failed"));

    let items = service.get_all().await.expect("history retrieval failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn supports_mixed_concurrent_operations() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = Arc::new(ClipboardHistoryService::new(
        repository,
        HistoryConfig { max_items: 30 },
    ));

    let first_id = uuid::Uuid::new_v4();

    let second_id = uuid::Uuid::new_v4();

    let first = ClipboardItem {
        id: first_id,
        content: ClipboardContent::Text("First".to_string()),
        hash: "mixed-first".to_string(),
        created_at: Utc::now(),
    };

    let second = ClipboardItem {
        id: second_id,
        content: ClipboardContent::Text("Second".to_string()),
        hash: "mixed-second".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(1),
    };

    service.save(first).await.expect("first save failed");

    service.save(second).await.expect("second save failed");

    let read_task = {
        let service = Arc::clone(&service);

        tokio::spawn(async move { service.get_all().await })
    };

    let delete_task = {
        let service = Arc::clone(&service);

        let id = first_id.to_string();

        tokio::spawn(async move { service.delete(&id).await })
    };

    let second_read_task = {
        let service = Arc::clone(&service);

        tokio::spawn(async move { service.get_all().await })
    };

    assert!(read_task.await.expect("read task failed").is_ok());

    assert!(delete_task.await.expect("delete task failed").is_ok());

    assert!(
        second_read_task
            .await
            .expect("second read task failed")
            .is_ok()
    );

    let items = service
        .get_all()
        .await
        .expect("final history retrieval failed");

    assert_eq!(items.len(), 1);

    assert_eq!(items[0].id, second_id.to_string(),);

    assert_eq!(items[0].text_content.as_deref(), Some("Second"),);
}

#[tokio::test]
async fn gets_history_item_by_id() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let item_id = uuid::Uuid::new_v4();

    let item = ClipboardItem {
        id: item_id,
        content: ClipboardContent::Text("Selected item".to_string()),
        hash: "selected-hash".to_string(),
        created_at: Utc::now(),
    };

    service.save(item).await.expect("save failed");

    let found = service
        .get_by_id(&item_id.to_string())
        .await
        .expect("lookup failed");

    let found = found.expect("item not found");

    assert_eq!(found.text_content.as_deref(), Some("Selected item"),);
}

#[tokio::test]
async fn returns_none_for_missing_history_item() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let found = service.get_by_id("missing").await.expect("lookup failed");

    assert!(found.is_none());
}

#[tokio::test]
async fn promotes_history_item_to_most_recent() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let base_time = Utc::now() - chrono::Duration::seconds(10);

    let a_id = uuid::Uuid::new_v4();
    let b_id = uuid::Uuid::new_v4();
    let c_id = uuid::Uuid::new_v4();

    let a = ClipboardItem {
        id: a_id,
        content: ClipboardContent::Text("A".to_string()),
        hash: "promote-a".to_string(),
        created_at: base_time,
    };

    let b = ClipboardItem {
        id: b_id,
        content: ClipboardContent::Text("B".to_string()),
        hash: "promote-b".to_string(),
        created_at: base_time + chrono::Duration::seconds(1),
    };

    let c = ClipboardItem {
        id: c_id,
        content: ClipboardContent::Text("C".to_string()),
        hash: "promote-c".to_string(),
        created_at: base_time + chrono::Duration::seconds(2),
    };

    service.save(a).await.expect("save A failed");
    service.save(b).await.expect("save B failed");
    service.save(c).await.expect("save C failed");

    let promoted = service
        .promote(&b_id.to_string())
        .await
        .expect("promotion failed");

    assert!(promoted);

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 3);

    assert_eq!(items[0].id, b_id.to_string());
    assert_eq!(items[1].id, c_id.to_string());
    assert_eq!(items[2].id, a_id.to_string());
}

#[tokio::test]
async fn promote_returns_false_for_missing_history_item() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let promoted = service.promote("missing").await.expect("promotion failed");

    assert!(!promoted);

    let items = service.get_all().await.expect("history retrieval failed");

    assert!(items.is_empty());
}

#[tokio::test]
async fn pins_and_unpins_history_item() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);
    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let id = uuid::Uuid::new_v4();
    let item = ClipboardItem {
        id,
        content: ClipboardContent::Text("Important note".to_string()),
        hash: "hash-pin".to_string(),
        created_at: Utc::now(),
    };

    service.save(item).await.expect("save failed");
    let item_id = id.to_string();

    let pinned = service.pin(&item_id).await.expect("pin failed");
    assert!(pinned);

    let fetched = service
        .get_by_id(&item_id)
        .await
        .expect("get failed")
        .unwrap();
    assert!(fetched.pinned_at.is_some());

    let unpinned = service.unpin(&item_id).await.expect("unpin failed");
    assert!(unpinned);

    let fetched = service
        .get_by_id(&item_id)
        .await
        .expect("get failed")
        .unwrap();
    assert_eq!(fetched.pinned_at, None);

    // Test toggle_pin
    let toggled_on = service
        .toggle_pin(&item_id)
        .await
        .expect("toggle pin failed");
    assert_eq!(toggled_on, Some(true));

    let fetched = service
        .get_by_id(&item_id)
        .await
        .expect("get failed")
        .unwrap();
    assert!(fetched.pinned_at.is_some());

    let toggled_off = service
        .toggle_pin(&item_id)
        .await
        .expect("toggle unpin failed");
    assert_eq!(toggled_off, Some(false));

    let fetched = service
        .get_by_id(&item_id)
        .await
        .expect("get failed")
        .unwrap();
    assert_eq!(fetched.pinned_at, None);

    let missing = service
        .toggle_pin("non-existent")
        .await
        .expect("toggle missing failed");
    assert_eq!(missing, None);
}

#[tokio::test]
async fn preserves_pinned_state_on_duplicate_text_save() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);
    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

    let id_a = uuid::Uuid::new_v4();
    let item_a = ClipboardItem {
        id: id_a,
        content: ClipboardContent::Text("My Password".to_string()),
        hash: "same-hash".to_string(),
        created_at: Utc::now(),
    };

    service.save(item_a).await.expect("save A failed");
    service.pin(&id_a.to_string()).await.expect("pin failed");

    let item_a_before = service.get_by_id(&id_a.to_string()).await.unwrap().unwrap();
    let original_pinned_at = item_a_before.pinned_at.clone();
    assert!(original_pinned_at.is_some());

    // User copies the exact same text again later
    let id_b = uuid::Uuid::new_v4();
    let item_b = ClipboardItem {
        id: id_b,
        content: ClipboardContent::Text("My Password".to_string()),
        hash: "same-hash".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(5),
    };

    service.save(item_b).await.expect("save B failed");

    let items = service.get_all().await.expect("get_all failed");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, id_b.to_string());
    assert_eq!(items[0].pinned_at, original_pinned_at);
}

#[tokio::test]
async fn preserves_pinned_state_on_duplicate_image_save() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let test_dir = std::env::temp_dir().join(format!("pookie-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&test_dir).expect("failed creating test dir");

    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(test_dir.clone());

    let image_store = storage::ImageStore::new(test_dir);

    let repository = StorageRepository::new(&database);
    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 })
        .with_image_store(image_store);

    let png_bytes = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG header bytes
    let id_a = uuid::Uuid::new_v4();
    let item_a = ClipboardItem {
        id: id_a,
        content: ClipboardContent::Image(png_bytes.clone()),
        hash: "same-image-hash".to_string(),
        created_at: Utc::now(),
    };

    service.save(item_a).await.expect("save image A failed");
    service.pin(&id_a.to_string()).await.expect("pin failed");

    let item_a_before = service.get_by_id(&id_a.to_string()).await.unwrap().unwrap();
    let original_pinned_at = item_a_before.pinned_at.clone();
    assert!(original_pinned_at.is_some());

    // Recopy same image
    let id_b = uuid::Uuid::new_v4();
    let item_b = ClipboardItem {
        id: id_b,
        content: ClipboardContent::Image(png_bytes),
        hash: "same-image-hash".to_string(),
        created_at: Utc::now() + chrono::Duration::seconds(5),
    };

    service.save(item_b).await.expect("save image B failed");

    let items = service.get_all().await.expect("get_all failed");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].pinned_at, original_pinned_at);
}

#[tokio::test]
async fn pinned_items_are_never_evicted_when_max_limit_is_reached() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);
    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 3 });

    let base_time = Utc::now();

    // 1. Create and pin item A (oldest)
    let id_a = uuid::Uuid::new_v4();
    let item_a = ClipboardItem {
        id: id_a,
        content: ClipboardContent::Text("Item A (Pinned)".to_string()),
        hash: "hash-a".to_string(),
        created_at: base_time,
    };
    service.save(item_a).await.expect("save A failed");
    service.pin(&id_a.to_string()).await.expect("pin A failed");

    // 2. Create item B and C (unpinned)
    let id_b = uuid::Uuid::new_v4();
    let item_b = ClipboardItem {
        id: id_b,
        content: ClipboardContent::Text("Item B (Unpinned)".to_string()),
        hash: "hash-b".to_string(),
        created_at: base_time + chrono::Duration::seconds(1),
    };
    service.save(item_b).await.expect("save B failed");

    let id_c = uuid::Uuid::new_v4();
    let item_c = ClipboardItem {
        id: id_c,
        content: ClipboardContent::Text("Item C (Unpinned)".to_string()),
        hash: "hash-c".to_string(),
        created_at: base_time + chrono::Duration::seconds(2),
    };
    service.save(item_c).await.expect("save C failed");

    // At capacity (3 items: A pinned, B, C)
    let items = service.get_all().await.expect("get_all failed");
    assert_eq!(items.len(), 3);

    // 3. Add item D (exceeds limit 3)
    let id_d = uuid::Uuid::new_v4();
    let item_d = ClipboardItem {
        id: id_d,
        content: ClipboardContent::Text("Item D (Newest)".to_string()),
        hash: "hash-d".to_string(),
        created_at: base_time + chrono::Duration::seconds(3),
    };
    service.save(item_d).await.expect("save D failed");

    let items = service.get_all().await.expect("get_all failed");
    assert_eq!(items.len(), 3);

    let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();

    // Expected: A is pinned (stays!), D is newest unpinned, C is next unpinned.
    // B was the oldest unpinned item and was evicted!
    assert!(ids.contains(&id_a.to_string().as_str()));
    assert!(ids.contains(&id_d.to_string().as_str()));
    assert!(ids.contains(&id_c.to_string().as_str()));
    assert!(!ids.contains(&id_b.to_string().as_str()));

    // Verify A remains pinned
    let item_a_fetched = service.get_by_id(&id_a.to_string()).await.unwrap().unwrap();
    assert!(item_a_fetched.pinned_at.is_some());
}

#[tokio::test]
async fn pin_persists_after_service_restart_simulation() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let id = uuid::Uuid::new_v4();
    let item_id = id.to_string();

    // First service instance
    {
        let repository = StorageRepository::new(&database);
        let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

        let item = ClipboardItem {
            id,
            content: ClipboardContent::Text("Persistent pinned item".to_string()),
            hash: "hash-persist".to_string(),
            created_at: Utc::now(),
        };

        service.save(item).await.expect("save failed");
        service.pin(&item_id).await.expect("pin failed");
    }

    // Simulate daemon/app restart: instantiate a brand new ClipboardHistoryService connected to same DB
    {
        let repository = StorageRepository::new(&database);
        let restarted_service =
            ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

        let items = restarted_service
            .get_all()
            .await
            .expect("fetch after restart failed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, item_id);
        assert!(items[0].pinned_at.is_some());
    }
}
