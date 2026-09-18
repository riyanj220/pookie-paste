use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Duration, Utc};

use history::{ClipboardHistoryService, HistoryConfig};

use pookie_clipboard::{ClipboardContent, canonicalize_rgba};

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
            "pookie-mixed-regression-test-{}-{timestamp}-{counter}",
            std::process::id(),
        ));

        fs::create_dir_all(&path).expect("failed creating test directory");

        Self { path }
    }

    fn database_path(&self) -> PathBuf {
        self.path.join("history.db")
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

async fn create_memory_service(
    max_items: usize,
) -> (ClipboardHistoryService, ImageStore, TestDirectory) {
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

async fn create_file_service(
    database_path: &Path,
    data_directory: &Path,
    max_items: usize,
) -> ClipboardHistoryService {
    let database_url = format!("sqlite://{}", database_path.display(),);

    let database = Database::new(&database_url)
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    ClipboardHistoryService::new(repository, HistoryConfig { max_items })
        .with_image_store(ImageStore::new(data_directory))
}

fn canonical_image(variant: u8) -> Vec<u8> {
    let pixels = match variant {
        1 => {
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ]
        }

        2 => {
            vec![
                20, 40, 60, 255, 80, 100, 120, 255, 140, 160, 180, 255, 200, 220, 240, 255,
            ]
        }

        _ => {
            vec![
                variant, variant, variant, 255, variant, 0, 0, 255, 0, variant, 0, 255, 0, 0,
                variant, 255,
            ]
        }
    };

    canonicalize_rgba(2, 2, &pixels).expect("failed creating canonical test image")
}

fn text_item(text: &str, hash: &str, created_at: chrono::DateTime<Utc>) -> ClipboardItem {
    ClipboardItem {
        id: uuid::Uuid::new_v4(),

        content: ClipboardContent::Text(text.to_string()),

        hash: hash.to_string(),

        created_at,
    }
}

fn image_item(image: Vec<u8>, hash: &str, created_at: chrono::DateTime<Utc>) -> ClipboardItem {
    ClipboardItem {
        id: uuid::Uuid::new_v4(),

        content: ClipboardContent::Image(image),

        hash: hash.to_string(),

        created_at,
    }
}

#[tokio::test]
async fn mixed_text_image_text_image_sequence_preserves_order_and_types() {
    let (service, image_store, _directory) = create_memory_service(30).await;

    let base = Utc::now() - Duration::seconds(20);

    let text_a = text_item("first text", "mixed-text-a", base);

    let image_a = image_item(
        canonical_image(1),
        "mixed-image-a",
        base + Duration::seconds(1),
    );

    let text_b = text_item("second text", "mixed-text-b", base + Duration::seconds(2));

    let image_b = image_item(
        canonical_image(2),
        "mixed-image-b",
        base + Duration::seconds(3),
    );

    let text_a_id = text_a.id.to_string();

    let image_a_id = image_a.id.to_string();

    let text_b_id = text_b.id.to_string();

    let image_b_id = image_b.id.to_string();

    service.save(text_a).await.expect("first text save failed");

    service
        .save(image_a)
        .await
        .expect("first image save failed");

    service.save(text_b).await.expect("second text save failed");

    service
        .save(image_b)
        .await
        .expect("second image save failed");

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 4,);

    assert_eq!(items[0].id, image_b_id,);

    assert_eq!(items[0].content_type, "image",);

    assert_eq!(items[1].id, text_b_id,);

    assert_eq!(items[1].content_type, "text",);

    assert_eq!(items[2].id, image_a_id,);

    assert_eq!(items[2].content_type, "image",);

    assert_eq!(items[3].id, text_a_id,);

    assert_eq!(items[3].content_type, "text",);

    for item in items.iter().filter(|item| item.content_type == "image") {
        assert!(item.text_content.is_none(),);

        let file_path = item.file_path.as_deref().expect("image file path missing");

        assert!(
            image_store
                .image_exists(file_path,)
                .await
                .expect("image existence check failed",),
        );
    }

    for item in items.iter().filter(|item| item.content_type == "text") {
        assert!(item.text_content.is_some(),);

        assert!(item.file_path.is_none(),);
    }
}

#[tokio::test]
async fn text_and_image_with_same_hash_do_not_cross_deduplicate() {
    let (service, image_store, _directory) = create_memory_service(30).await;

    let shared_hash = "intentionally-shared-hash";

    let base = Utc::now();

    let text = text_item("same hash text", shared_hash, base);

    let image = image_item(canonical_image(1), shared_hash, base + Duration::seconds(1));

    let text_id = text.id.to_string();

    let image_id = image.id.to_string();

    service.save(text).await.expect("text save failed");

    service.save(image).await.expect("image save failed");

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 2, "text and image hashes must be type-scoped",);

    assert!(
        items
            .iter()
            .any(|item| { item.id == text_id && item.content_type == "text" },),
    );

    assert!(
        items
            .iter()
            .any(|item| { item.id == image_id && item.content_type == "image" },),
    );

    assert!(
        image_store
            .image_exists(&format!("images/{image_id}.png"),)
            .await
            .expect("image existence check failed",),
    );
}

#[tokio::test]
async fn deleting_image_from_mixed_history_preserves_text_items() {
    let (service, image_store, _directory) = create_memory_service(30).await;

    let base = Utc::now();

    let text_before = text_item("before", "delete-mixed-text-a", base);

    let image = image_item(
        canonical_image(1),
        "delete-mixed-image",
        base + Duration::seconds(1),
    );

    let text_after = text_item("after", "delete-mixed-text-b", base + Duration::seconds(2));

    let text_before_id = text_before.id.to_string();

    let image_id = image.id.to_string();

    let text_after_id = text_after.id.to_string();

    service
        .save(text_before)
        .await
        .expect("first text save failed");

    service.save(image).await.expect("image save failed");

    service
        .save(text_after)
        .await
        .expect("second text save failed");

    let image_path = format!("images/{image_id}.png");

    assert!(
        image_store
            .image_exists(&image_path,)
            .await
            .expect("image existence check failed",),
    );

    assert!(
        service
            .delete(&image_id,)
            .await
            .expect("image delete failed",),
    );

    assert!(
        !image_store
            .image_exists(&image_path,)
            .await
            .expect("image existence check failed",),
    );

    let remaining = service.get_all().await.expect("history retrieval failed");

    assert_eq!(remaining.len(), 2,);

    assert!(remaining.iter().any(|item| { item.id == text_before_id },),);

    assert!(remaining.iter().any(|item| { item.id == text_after_id },),);

    assert!(
        remaining
            .iter()
            .all(|item| { item.content_type == "text" },),
    );
}

#[tokio::test]
async fn clearing_mixed_history_removes_rows_and_owned_image_files() {
    let (service, image_store, _directory) = create_memory_service(30).await;

    let base = Utc::now();

    let text = text_item("clear me", "clear-mixed-text", base);

    let image_a = image_item(
        canonical_image(1),
        "clear-mixed-image-a",
        base + Duration::seconds(1),
    );

    let image_b = image_item(
        canonical_image(2),
        "clear-mixed-image-b",
        base + Duration::seconds(2),
    );

    let image_a_id = image_a.id.to_string();

    let image_b_id = image_b.id.to_string();

    service.save(text).await.expect("text save failed");

    service
        .save(image_a)
        .await
        .expect("first image save failed");

    service
        .save(image_b)
        .await
        .expect("second image save failed");

    let deleted = service.clear().await.expect("clear failed");

    assert_eq!(deleted, 3,);

    assert!(
        service
            .get_all()
            .await
            .expect("history retrieval failed",)
            .is_empty(),
    );

    for id in [image_a_id, image_b_id] {
        assert!(
            !image_store
                .image_exists(&format!("images/{id}.png"),)
                .await
                .expect("image existence check failed",),
        );
    }
}

#[tokio::test]
async fn mixed_history_limit_eviction_removes_evicted_image_file() {
    let (service, image_store, _directory) = create_memory_service(2).await;

    let base = Utc::now();

    let oldest_image = image_item(canonical_image(1), "limit-old-image", base);

    let text = text_item("middle text", "limit-text", base + Duration::seconds(1));

    let newest_image = image_item(
        canonical_image(2),
        "limit-new-image",
        base + Duration::seconds(2),
    );

    let oldest_image_id = oldest_image.id.to_string();

    let text_id = text.id.to_string();

    let newest_image_id = newest_image.id.to_string();

    service
        .save(oldest_image)
        .await
        .expect("old image save failed");

    service.save(text).await.expect("text save failed");

    service
        .save(newest_image)
        .await
        .expect("new image save failed");

    let items = service.get_all().await.expect("history retrieval failed");

    assert_eq!(items.len(), 2,);

    assert!(items.iter().any(|item| { item.id == text_id },),);

    assert!(items.iter().any(|item| { item.id == newest_image_id },),);

    assert!(items.iter().all(|item| { item.id != oldest_image_id },),);

    assert!(
        !image_store
            .image_exists(&format!("images/{oldest_image_id}.png"),)
            .await
            .expect("old image existence check failed",),
        "evicted image file leaked",
    );

    assert!(
        image_store
            .image_exists(&format!("images/{newest_image_id}.png"),)
            .await
            .expect("new image existence check failed",),
    );
}

#[tokio::test]
async fn mixed_history_survives_database_restart_with_image_reference_intact() {
    let directory = TestDirectory::new();

    let database_path = directory.database_path();

    let base = Utc::now();

    let text_id;
    let image_id;
    let image_path;
    let canonical = canonical_image(1);

    {
        let service = create_file_service(&database_path, &directory.path, 30).await;

        let text = text_item("persistent text", "restart-text", base);

        let image = image_item(
            canonical.clone(),
            "restart-image",
            base + Duration::seconds(1),
        );

        text_id = text.id.to_string();

        image_id = image.id.to_string();

        image_path = format!("images/{image_id}.png");

        service.save(text).await.expect("text save failed");

        service.save(image).await.expect("image save failed");

        assert_eq!(
            service
                .get_all()
                .await
                .expect("history retrieval failed",)
                .len(),
            2,
        );
    }

    /*
     * Recreate Database + Repository + HistoryService from
     * the exact same on-disk database and data directory.
     */
    let restarted = create_file_service(&database_path, &directory.path, 30).await;

    let items = restarted
        .get_all()
        .await
        .expect("history retrieval after restart failed");

    assert_eq!(items.len(), 2,);

    assert!(
        items
            .iter()
            .any(|item| { item.id == text_id && item.content_type == "text" },),
    );

    let stored_image = items
        .iter()
        .find(|item| item.id == image_id)
        .expect("persisted image row missing");

    assert_eq!(stored_image.content_type, "image",);

    assert_eq!(
        stored_image.file_path.as_deref(),
        Some(image_path.as_str(),),
    );

    let restored_bytes = restarted
        .read_image_content(&image_path)
        .await
        .expect("persisted image read failed")
        .expect("image store unexpectedly unavailable");

    assert_eq!(restored_bytes, canonical,);

    let removed = restarted
        .reconcile_image_store()
        .await
        .expect("reconciliation failed");

    assert_eq!(
        removed, 0,
        "referenced image must survive restart reconciliation",
    );
}

#[tokio::test]
async fn missing_referenced_image_file_returns_error_without_removing_history_row() {
    let (service, image_store, _directory) = create_memory_service(30).await;

    let image = image_item(canonical_image(1), "missing-file-image", Utc::now());

    let image_id = image.id.to_string();

    service.save(image).await.expect("image save failed");

    let row = service
        .get_by_id(&image_id)
        .await
        .expect("history lookup failed")
        .expect("image row missing");

    let file_path = row.file_path.clone().expect("image file path missing");

    image_store
        .delete_image(&file_path)
        .await
        .expect("failed deleting test image");

    assert!(
        service.read_image_content(&file_path,).await.is_err(),
        "missing image file should produce a safe error",
    );

    /*
     * A transient/missing file must not silently destroy the
     * history row during a read attempt.
     */
    assert!(
        service
            .get_by_id(&image_id,)
            .await
            .expect("history lookup failed",)
            .is_some(),
    );
}

#[tokio::test]
async fn startup_reconciliation_removes_orphan_but_keeps_referenced_image() {
    let (service, image_store, _directory) = create_memory_service(30).await;

    let referenced = image_item(canonical_image(1), "reconcile-referenced", Utc::now());

    let referenced_id = referenced.id.to_string();

    service
        .save(referenced)
        .await
        .expect("referenced image save failed");

    let referenced_path = format!("images/{referenced_id}.png");

    let orphan_id = uuid::Uuid::new_v4();

    let orphan_path = image_store
        .write_image(&orphan_id.to_string(), &canonical_image(2))
        .await
        .expect("orphan image write failed");

    let removed = service
        .reconcile_image_store()
        .await
        .expect("reconciliation failed");

    assert_eq!(removed, 1,);

    assert!(
        image_store
            .image_exists(&referenced_path,)
            .await
            .expect("referenced image existence check failed",),
    );

    assert!(
        !image_store
            .image_exists(&orphan_path,)
            .await
            .expect("orphan image existence check failed",),
    );
}
