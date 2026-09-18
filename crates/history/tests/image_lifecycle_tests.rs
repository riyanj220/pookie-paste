use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Duration, Utc};

use history::{ClipboardHistoryService, HistoryConfig};
use pookie_clipboard::ClipboardContent;
use pookie_core::ClipboardItem;
use storage::{Database, ImageStore, StorageRepository};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();

        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);

        let path = std::env::temp_dir().join(format!(
            "pookie-history-image-test-{}-{timestamp}-{counter}",
            std::process::id(),
        ));

        fs::create_dir_all(&path).expect("failed creating test directory");

        Self { path }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

async fn create_service(max_items: usize) -> (ClipboardHistoryService, ImageStore, TestDirectory) {
    let directory = TestDirectory::new();

    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let image_store = ImageStore::new(&directory.path);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items })
        .with_image_store(image_store.clone());

    (service, image_store, directory)
}

fn image_item(
    id: uuid::Uuid,
    hash: &str,
    bytes: Vec<u8>,
    created_at: chrono::DateTime<Utc>,
) -> ClipboardItem {
    ClipboardItem {
        id,

        content: ClipboardContent::Image(bytes),

        hash: hash.to_string(),

        created_at,
    }
}

#[tokio::test]
async fn saving_image_creates_owned_file_and_database_reference() {
    let (service, image_store, _directory) = create_service(30).await;

    let id = uuid::Uuid::new_v4();

    service
        .save(image_item(id, "image-hash", vec![1, 2, 3, 4], Utc::now()))
        .await
        .expect("image save failed");

    let stored = service
        .get_by_id(&id.to_string())
        .await
        .expect("history lookup failed")
        .expect("image row missing");

    assert_eq!(stored.content_type, "image",);

    assert!(stored.text_content.is_none(),);

    let file_path = stored.file_path.expect("image file path missing");

    assert_eq!(file_path, format!("images/{id}.png"),);

    assert!(
        image_store
            .image_exists(&file_path,)
            .await
            .expect("image existence check failed",),
    );

    assert_eq!(
        image_store
            .read_image(&file_path,)
            .await
            .expect("image read failed",),
        vec![1, 2, 3, 4],
    );
}

#[tokio::test]
async fn duplicate_image_reuses_existing_owned_file() {
    let (service, image_store, _directory) = create_service(30).await;

    let first_id = uuid::Uuid::new_v4();

    let second_id = uuid::Uuid::new_v4();

    let base_time = Utc::now() - Duration::seconds(10);

    service
        .save(image_item(
            first_id,
            "duplicate-image-hash",
            vec![1, 2, 3, 4],
            base_time,
        ))
        .await
        .expect("first image save failed");

    service
        .save(image_item(
            second_id,
            "duplicate-image-hash",
            vec![1, 2, 3, 4],
            base_time + Duration::seconds(5),
        ))
        .await
        .expect("duplicate image save failed");

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 1,);

    assert_eq!(items[0].id, first_id.to_string(),);

    let first_path = format!("images/{first_id}.png");

    let second_path = format!("images/{second_id}.png");

    assert!(
        image_store
            .image_exists(&first_path,)
            .await
            .expect("first image existence check failed",),
    );

    assert!(
        !image_store
            .image_exists(&second_path,)
            .await
            .expect("second image existence check failed",),
    );
}

#[tokio::test]
async fn deleting_image_removes_database_row_and_file() {
    let (service, image_store, _directory) = create_service(30).await;

    let id = uuid::Uuid::new_v4();

    service
        .save(image_item(
            id,
            "delete-image-hash",
            vec![5, 6, 7],
            Utc::now(),
        ))
        .await
        .expect("image save failed");

    let file_path = format!("images/{id}.png");

    let deleted = service
        .delete(&id.to_string())
        .await
        .expect("history delete failed");

    assert!(deleted);

    assert!(
        service
            .get_by_id(&id.to_string(),)
            .await
            .expect("history lookup failed",)
            .is_none(),
    );

    assert!(
        !image_store
            .image_exists(&file_path,)
            .await
            .expect("image existence check failed",),
    );
}

#[tokio::test]
async fn clear_history_removes_all_image_files() {
    let (service, image_store, _directory) = create_service(30).await;

    let first_id = uuid::Uuid::new_v4();

    let second_id = uuid::Uuid::new_v4();

    service
        .save(image_item(first_id, "clear-image-a", vec![1], Utc::now()))
        .await
        .expect("first image save failed");

    service
        .save(image_item(
            second_id,
            "clear-image-b",
            vec![2],
            Utc::now() + Duration::seconds(1),
        ))
        .await
        .expect("second image save failed");

    let deleted = service.clear().await.expect("clear history failed");

    assert_eq!(deleted, 2,);

    for id in [first_id, second_id] {
        let path = format!("images/{id}.png");

        assert!(
            !image_store
                .image_exists(&path,)
                .await
                .expect("image existence check failed",),
        );
    }
}

#[tokio::test]
async fn history_limit_eviction_removes_old_image_file() {
    let (service, image_store, _directory) = create_service(1).await;

    let first_id = uuid::Uuid::new_v4();

    let second_id = uuid::Uuid::new_v4();

    let base_time = Utc::now();

    service
        .save(image_item(first_id, "eviction-image-a", vec![1], base_time))
        .await
        .expect("first image save failed");

    service
        .save(image_item(
            second_id,
            "eviction-image-b",
            vec![2],
            base_time + Duration::seconds(1),
        ))
        .await
        .expect("second image save failed");

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 1,);

    assert_eq!(items[0].id, second_id.to_string(),);

    assert!(
        !image_store
            .image_exists(&format!("images/{first_id}.png"),)
            .await
            .expect("first image existence check failed",),
    );

    assert!(
        image_store
            .image_exists(&format!("images/{second_id}.png"),)
            .await
            .expect("second image existence check failed",),
    );
}

#[tokio::test]
async fn failed_database_insert_rolls_back_new_image_file() {
    let (service, image_store, _directory) = create_service(30).await;

    let shared_id = uuid::Uuid::new_v4();

    let text_item = ClipboardItem {
        id: shared_id,

        content: ClipboardContent::Text("existing row".to_string()),

        hash: "existing-text-hash".to_string(),

        created_at: Utc::now(),
    };

    service.save(text_item).await.expect("text save failed");

    let result = service
        .save(image_item(
            shared_id,
            "new-image-hash",
            vec![9, 9, 9],
            Utc::now() + Duration::seconds(1),
        ))
        .await;

    assert!(result.is_err(), "duplicate primary key should fail",);

    let image_path = format!("images/{shared_id}.png");

    assert!(
        !image_store
            .image_exists(&image_path,)
            .await
            .expect("image existence check failed",),
        "failed database insert leaked image file",
    );
}

#[tokio::test]
async fn reconciliation_removes_unreferenced_image_file() {
    let (service, image_store, _directory) = create_service(30).await;

    let orphan_id = uuid::Uuid::new_v4();

    let orphan_path = image_store
        .write_image(&orphan_id.to_string(), &[1, 2, 3])
        .await
        .expect("orphan image write failed");

    assert!(
        image_store
            .image_exists(&orphan_path,)
            .await
            .expect("orphan existence check failed",),
    );

    let removed = service
        .reconcile_image_store()
        .await
        .expect("image reconciliation failed");

    assert_eq!(removed, 1,);

    assert!(
        !image_store
            .image_exists(&orphan_path,)
            .await
            .expect("orphan existence check failed",),
    );
}

#[tokio::test]
async fn reconciliation_keeps_referenced_image_file() {
    let (service, image_store, _directory) = create_service(30).await;

    let id = uuid::Uuid::new_v4();

    service
        .save(image_item(
            id,
            "referenced-image-hash",
            vec![1, 2, 3],
            Utc::now(),
        ))
        .await
        .expect("image save failed");

    let file_path = format!("images/{id}.png");

    let removed = service
        .reconcile_image_store()
        .await
        .expect("image reconciliation failed");

    assert_eq!(removed, 0,);

    assert!(
        image_store
            .image_exists(&file_path,)
            .await
            .expect("image existence check failed",),
    );
}
