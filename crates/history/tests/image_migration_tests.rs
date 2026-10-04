use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Utc;
use history::{ClipboardHistoryService, HistoryConfig};
use pookie_clipboard::{CanonicalImage, ClipboardContent, canonicalize_rgba};
use pookie_core::ClipboardItem;
use storage::{Database, ImageStore, StorageRepository, StoredClipboardItem};

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
            "pookie-migration-test-{}-{timestamp}-{counter}",
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

async fn create_service_with_repo(
    max_items: usize,
) -> (
    ClipboardHistoryService,
    StorageRepository,
    ImageStore,
    TestDirectory,
) {
    let directory = TestDirectory::new();

    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let image_store = ImageStore::new(&directory.path);

    let service = ClipboardHistoryService::new(repository.clone(), HistoryConfig { max_items })
        .with_image_store(image_store.clone());

    (service, repository, image_store, directory)
}

fn test_canonical_image(seed: u8) -> CanonicalImage {
    canonicalize_rgba(1, 1, &[seed, seed, seed, 255]).expect("canonical image failed")
}

async fn insert_legacy_row(
    repository: &StorageRepository,
    image_store: &ImageStore,
    id: &str,
    png_bytes: &[u8],
    legacy_hash: &str,
    created_at: &str,
    pinned_at: Option<&str>,
) -> String {
    let file_path = image_store.write_image(id, png_bytes).await.unwrap();
    let item = StoredClipboardItem {
        id: id.to_string(),
        content_type: "image".to_string(),
        text_content: None,
        file_path: Some(file_path.clone()),
        content_hash: legacy_hash.to_string(),
        created_at: created_at.to_string(),
        pinned_at: pinned_at.map(String::from),
    };
    repository.insert(&item).await.unwrap();
    file_path
}

// 1. Single legacy row migrates successfully
#[tokio::test]
async fn single_legacy_migration_succeeds() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(10);
    let legacy_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-1",
        image.png_bytes(),
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 1);
    assert_eq!(summary.updated_in_place, 1);
    assert_eq!(summary.consolidated, 0);

    let updated = repository.get_by_id("item-1").await.unwrap().unwrap();
    assert_eq!(updated.content_hash, image.identity().to_versioned_string());
    assert_eq!(updated.file_path, Some("images/item-1.png".to_string()));
    assert!(image_store.image_exists("images/item-1.png").await.unwrap());
}

// 2. Already rgba-v1 row is skipped
#[tokio::test]
async fn already_migrated_rgba_v1_skipped() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(20);
    let rgba_v1_hash = image.identity().to_versioned_string();

    insert_legacy_row(
        &repository,
        &image_store,
        "item-already",
        image.png_bytes(),
        &rgba_v1_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 0);
    assert_eq!(summary.updated_in_place, 0);

    let row = repository.get_by_id("item-already").await.unwrap().unwrap();
    assert_eq!(row.content_hash, rgba_v1_hash);
}

// 3. Future-version hash skipped
#[tokio::test]
async fn future_version_hash_skipped() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(30);

    insert_legacy_row(
        &repository,
        &image_store,
        "item-future",
        image.png_bytes(),
        "rgba-v2:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 0);
}

// 4. Malformed legacy hash skipped
#[tokio::test]
async fn malformed_legacy_hash_skipped() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(40);

    // Hash length is 63 (invalid)
    insert_legacy_row(
        &repository,
        &image_store,
        "item-malformed",
        image.png_bytes(),
        "short_hash_123",
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 0);
}

// 5. Missing backing file -> WARN + unchanged row
#[tokio::test]
async fn missing_backing_file_warns_and_leaves_row_unchanged() {
    let (service, repository, _image_store, _dir) = create_service_with_repo(30).await;
    let legacy_hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let item = StoredClipboardItem {
        id: "item-missing-file".to_string(),
        content_type: "image".to_string(),
        text_content: None,
        file_path: Some("images/non_existent.png".to_string()),
        content_hash: legacy_hash.to_string(),
        created_at: "2026-10-01T12:00:00Z".to_string(),
        pinned_at: None,
    };
    repository.insert(&item).await.unwrap();

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 1);
    assert_eq!(summary.skipped_corrupt_or_missing, 1);
    assert_eq!(summary.updated_in_place, 0);

    let row = repository
        .get_by_id("item-missing-file")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.content_hash, legacy_hash);
}

// 6. Corrupt PNG -> WARN + unchanged row
#[tokio::test]
async fn corrupt_png_warns_and_leaves_row_unchanged() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let legacy_hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-corrupt",
        b"not a valid png file payload",
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 1);
    assert_eq!(summary.skipped_corrupt_or_missing, 1);
    assert_eq!(summary.updated_in_place, 0);

    let row = repository.get_by_id("item-corrupt").await.unwrap().unwrap();
    assert_eq!(row.content_hash, legacy_hash);
}

// 7. Migration is idempotent
#[tokio::test]
async fn migration_is_idempotent() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(50);
    let legacy_hash = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-idempotent",
        image.png_bytes(),
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let summary1 = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary1.updated_in_place, 1);

    let summary2 = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary2.candidates_found, 0);
    assert_eq!(summary2.updated_in_place, 0);
}

// 8. Two legacy rows collapse to same rgba-v1
#[tokio::test]
async fn two_legacy_rows_collapse_to_same_rgba_v1() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(60);
    let legacy_hash_a = "1111111111111111111111111111111111111111111111111111111111111111";
    let legacy_hash_b = "2222222222222222222222222222222222222222222222222222222222222222";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-a",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "item-b",
        image.png_bytes(),
        legacy_hash_b,
        "2026-10-01T12:05:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 2);
    assert_eq!(summary.updated_in_place, 1);
    assert_eq!(summary.consolidated, 1);

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].content_hash, image.identity().to_versioned_string());
}

// 9. Pinned + unpinned collision preserves pinned state
#[tokio::test]
async fn pinned_plus_unpinned_collision_preserves_pinned_state() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(70);
    let legacy_hash_a = "3333333333333333333333333333333333333333333333333333333333333333";
    let legacy_hash_b = "4444444444444444444444444444444444444444444444444444444444444444";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-pinned",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:00:00Z",
        Some("2026-10-01T12:10:00Z"),
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "item-unpinned",
        image.png_bytes(),
        legacy_hash_b,
        "2026-10-01T12:05:00Z",
        None,
    )
    .await;

    service.migrate_legacy_image_identities().await.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, "item-pinned");
    assert_eq!(all[0].pinned_at, Some("2026-10-01T12:10:00Z".to_string()));
}

// 10. Both pinned collision preserves max pinned_at
#[tokio::test]
async fn both_pinned_collision_preserves_max_pinned_at() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(80);
    let legacy_hash_a = "5555555555555555555555555555555555555555555555555555555555555555";
    let legacy_hash_b = "6666666666666666666666666666666666666666666666666666666666666666";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-pin1",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:00:00Z",
        Some("2026-10-01T12:10:00Z"),
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "item-pin2",
        image.png_bytes(),
        legacy_hash_b,
        "2026-10-01T12:05:00Z",
        Some("2026-10-01T12:20:00Z"),
    )
    .await;

    service.migrate_legacy_image_identities().await.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].pinned_at, Some("2026-10-01T12:20:00Z".to_string()));
}

// 11. Collision preserves most recent created_at
#[tokio::test]
async fn collision_preserves_most_recent_created_at() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(90);
    let legacy_hash_a = "7777777777777777777777777777777777777777777777777777777777777777";
    let legacy_hash_b = "8888888888888888888888888888888888888888888888888888888888888888";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-old",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "item-new",
        image.png_bytes(),
        legacy_hash_b,
        "2026-10-01T12:30:00Z",
        None,
    )
    .await;

    service.migrate_legacy_image_identities().await.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].created_at, "2026-10-01T12:30:00Z");
}

// 12. Redundant file removed after successful commit
#[tokio::test]
async fn redundant_file_removed_after_successful_commit() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(100);
    let legacy_hash_a = "9999999999999999999999999999999999999999999999999999999999999999";
    let legacy_hash_b = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1";

    let file_a = insert_legacy_row(
        &repository,
        &image_store,
        "item-survivor",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:30:00Z",
        None,
    )
    .await;

    let file_b = insert_legacy_row(
        &repository,
        &image_store,
        "item-redundant",
        image.png_bytes(),
        legacy_hash_b,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    assert!(image_store.image_exists(&file_a).await.unwrap());
    assert!(image_store.image_exists(&file_b).await.unwrap());

    service.migrate_legacy_image_identities().await.unwrap();

    assert!(image_store.image_exists(&file_a).await.unwrap());
    assert!(!image_store.image_exists(&file_b).await.unwrap());
}

// 13. Cleanup failure does not corrupt DB
#[tokio::test]
async fn cleanup_failure_does_not_corrupt_db() {
    let (service, repository, image_store, dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(110);
    let legacy_hash_a = "baaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let target_rgba_v1 = image.identity().to_versioned_string();

    insert_legacy_row(
        &repository,
        &image_store,
        "item-will-survive",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:30:00Z",
        None,
    )
    .await;

    // For redundant item, create an existing colliding rgba-v1 row whose file_path points to
    // an existing directory on disk. This causes post-commit cleanup (`remove_file`) to fail
    // with EISDIR while the database consolidation transaction has already committed.
    let redundant_rel = "images/item-will-fail-cleanup.png";
    let redundant_abs = dir.path.join(redundant_rel);
    fs::create_dir_all(&redundant_abs).unwrap();

    let item_b = StoredClipboardItem {
        id: "item-will-fail-cleanup".to_string(),
        content_type: "image".to_string(),
        text_content: None,
        file_path: Some(redundant_rel.to_string()),
        content_hash: target_rgba_v1.clone(),
        created_at: "2026-10-01T12:00:00Z".to_string(),
        pinned_at: None,
    };
    repository.insert(&item_b).await.unwrap();

    // Migration logs a warning on file cleanup failure but completes successfully
    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.consolidated, 1);

    // Database state remains committed and consistent: item_b was deleted, survivor row remains
    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, "item-will-survive");
    assert_eq!(all[0].content_hash, target_rgba_v1);
    assert!(
        image_store
            .image_exists("images/item-will-survive.png")
            .await
            .unwrap()
    );

    // Clean up directory to satisfy TempDir drop
    let _ = fs::remove_dir(&redundant_abs);
}

// 14. Candidate deleted before transaction -> skipped, no resurrection
#[tokio::test]
async fn candidate_deleted_before_transaction_skipped() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(120);
    let legacy_hash = "caaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-to-delete",
        image.png_bytes(),
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    // Delete row before migration transaction
    repository.delete_by_id("item-to-delete").await.unwrap();

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 0);

    let all = repository.get_all().await.unwrap();
    assert!(all.is_empty());
}

// 15. Candidate hash changed before transaction -> skipped
#[tokio::test]
async fn candidate_hash_changed_before_transaction_skipped() {
    let (_service, repository, _image_store, _dir) = create_service_with_repo(30).await;
    let legacy_hash = "daaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let new_hash = "rgba-v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    let item = StoredClipboardItem {
        id: "item-hash-change".to_string(),
        content_type: "image".to_string(),
        text_content: None,
        file_path: Some("images/item.png".to_string()),
        content_hash: legacy_hash.to_string(),
        created_at: "2026-10-01T12:00:00Z".to_string(),
        pinned_at: None,
    };
    repository.insert(&item).await.unwrap();

    let mut healthy_ids = std::collections::HashSet::new();
    healthy_ids.insert("item-hash-change".to_string());

    // Try consolidate with mismatched expected hash
    let outcome = repository
        .consolidate_legacy_image_row(
            "item-hash-change",
            "wrong_old_hash_00000000000000000000000000000000000000000000000000000000",
            new_hash,
            &healthy_ids,
        )
        .await
        .unwrap();

    assert_eq!(outcome, storage::ImageMigrationOutcome::SkippedStale);
}

// 16. Collision with already-existing rgba-v1 runtime row
#[tokio::test]
async fn collision_with_already_existing_rgba_v1_runtime_row() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(130);
    let legacy_hash = "eaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let target_rgba_v1 = image.identity().to_versioned_string();

    insert_legacy_row(
        &repository,
        &image_store,
        "item-legacy",
        image.png_bytes(),
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "item-runtime",
        image.png_bytes(),
        &target_rgba_v1,
        "2026-10-01T12:15:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 1);
    assert_eq!(summary.consolidated, 1);

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, "item-runtime");
    assert_eq!(all[0].content_hash, target_rgba_v1);
}

// 17. Clear-history race
#[tokio::test]
async fn clear_history_race_safely_handled() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(140);
    let legacy_hash = "faaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-to-clear",
        image.png_bytes(),
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    service.clear().await.unwrap();

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 0);

    let all = repository.get_all().await.unwrap();
    assert!(all.is_empty());
}

// 18. Concurrent runtime save during migration produces no duplicates
#[tokio::test]
async fn concurrent_runtime_save_during_migration_produces_no_duplicates() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(150);
    let legacy_hash = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-pre-existing",
        image.png_bytes(),
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let service = std::sync::Arc::new(service);
    let s1 = service.clone();
    let image_clone = image.clone();

    let migration_handle =
        tokio::spawn(async move { s1.migrate_legacy_image_identities().await.unwrap() });

    let s2 = service.clone();
    let runtime_save_handle = tokio::spawn(async move {
        let item = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Image(image_clone.clone()),
            hash: image_clone.identity().to_versioned_string(),
            created_at: Utc::now(),
        };
        s2.save(item).await.unwrap()
    });

    let (summary, save_outcome) = tokio::join!(migration_handle, runtime_save_handle);
    summary.unwrap();
    let _ = save_outcome.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].content_hash, image.identity().to_versioned_string());
}

// 19. Survivor selection prefers pinned row over unpinned candidate
#[tokio::test]
async fn survivor_selection_prefers_pinned_row_over_unpinned_candidate() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(160);
    let legacy_hash_a = "0111111111111111111111111111111111111111111111111111111111111111";
    let legacy_hash_b = "0222222222222222222222222222222222222222222222222222222222222222";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-candidate-a",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "item-candidate-b",
        image.png_bytes(),
        legacy_hash_b,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    // Pin row B with an explicit pin timestamp
    service.pin("item-candidate-b").await.unwrap();

    service.migrate_legacy_image_identities().await.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    // B was pinned so it must be chosen as the survivor
    assert_eq!(all[0].id, "item-candidate-b");
    assert!(all[0].pinned_at.is_some());
}

// 19b. Concurrent pin race with migration preserves pinned survivor
#[tokio::test]
async fn concurrent_pin_race_with_migration_preserves_pinned_survivor() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(161);
    let legacy_hash_a = "1111111111111111111111111111111111111111111111111111111111111122";
    let legacy_hash_b = "2222222222222222222222222222222222222222222222222222222222222233";

    insert_legacy_row(
        &repository,
        &image_store,
        "item-race-a",
        image.png_bytes(),
        legacy_hash_a,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "item-race-b",
        image.png_bytes(),
        legacy_hash_b,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let service = std::sync::Arc::new(service);
    let s1 = service.clone();
    let migration_handle =
        tokio::spawn(async move { s1.migrate_legacy_image_identities().await.unwrap() });

    let s2 = service.clone();
    let pin_handle = tokio::spawn(async move { s2.pin("item-race-b").await.unwrap() });

    let (summary, pin_result) = tokio::join!(migration_handle, pin_handle);
    summary.unwrap();
    let _ = pin_result.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].content_hash, image.identity().to_versioned_string());
    assert!(all[0].pinned_at.is_some());
}

// 20. Multiple target duplicates consolidate to single survivor
#[tokio::test]
async fn multiple_target_duplicates_consolidate_to_single_survivor() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(170);

    let h1 = "a111111111111111111111111111111111111111111111111111111111111111";
    let h2 = "a222222222222222222222222222222222222222222222222222222222222222";
    let h3 = "a333333333333333333333333333333333333333333333333333333333333333";

    insert_legacy_row(
        &repository,
        &image_store,
        "dup-1",
        image.png_bytes(),
        h1,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "dup-2",
        image.png_bytes(),
        h2,
        "2026-10-01T12:10:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "dup-3",
        image.png_bytes(),
        h3,
        "2026-10-01T12:20:00Z",
        None,
    )
    .await;

    service.migrate_legacy_image_identities().await.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].content_hash, image.identity().to_versioned_string());
    assert_eq!(all[0].created_at, "2026-10-01T12:20:00Z");
}

// 21. Corrupt rgba-v1 collision replaced by healthy candidate
#[tokio::test]
async fn corrupt_rgba_v1_collision_replaced_by_healthy_candidate() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(180);
    let legacy_hash = "b111111111111111111111111111111111111111111111111111111111111111";
    let target_hash = image.identity().to_versioned_string();

    // Valid legacy candidate
    insert_legacy_row(
        &repository,
        &image_store,
        "item-healthy-legacy",
        image.png_bytes(),
        legacy_hash,
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    // Colliding rgba-v1 row with missing backing file
    let corrupt_rgba_item = StoredClipboardItem {
        id: "item-ghost-rgba-v1".to_string(),
        content_type: "image".to_string(),
        text_content: None,
        file_path: Some("images/missing_ghost.png".to_string()),
        content_hash: target_hash.clone(),
        created_at: "2026-10-01T12:30:00Z".to_string(),
        pinned_at: Some("2026-10-01T12:40:00Z".to_string()),
    };
    repository.insert(&corrupt_rgba_item).await.unwrap();

    service.migrate_legacy_image_identities().await.unwrap();

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 1);
    // Healthy legacy item must survive because ghost row has no valid file
    assert_eq!(all[0].id, "item-healthy-legacy");
    assert_eq!(all[0].content_hash, target_hash);
    // Timestamps from ghost row are merged
    assert_eq!(all[0].created_at, "2026-10-01T12:30:00Z");
    assert_eq!(all[0].pinned_at, Some("2026-10-01T12:40:00Z".to_string()));
    // Valid backing file is preserved
    assert!(
        image_store
            .image_exists("images/item-healthy-legacy.png")
            .await
            .unwrap()
    );
}

// 22. Multiple legacy candidates migrate sequentially in batch
#[tokio::test]
async fn multiple_legacy_candidates_migrate_sequentially_in_batch() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image1 = test_canonical_image(190);
    let image2 = test_canonical_image(191);
    let image3 = test_canonical_image(192);

    insert_legacy_row(
        &repository,
        &image_store,
        "seq-1",
        image1.png_bytes(),
        "c111111111111111111111111111111111111111111111111111111111111111",
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "seq-2",
        image2.png_bytes(),
        "c222222222222222222222222222222222222222222222222222222222222222",
        "2026-10-01T12:01:00Z",
        None,
    )
    .await;

    insert_legacy_row(
        &repository,
        &image_store,
        "seq-3",
        image3.png_bytes(),
        "c333333333333333333333333333333333333333333333333333333333333333",
        "2026-10-01T12:02:00Z",
        None,
    )
    .await;

    let summary = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary.candidates_found, 3);
    assert_eq!(summary.updated_in_place, 3);
    assert_eq!(summary.consolidated, 0);

    let all = repository.get_all().await.unwrap();
    assert_eq!(all.len(), 3);
}

// 23. Migration re-run performs zero logical changes
#[tokio::test]
async fn migration_rerun_performs_zero_changes() {
    let (service, repository, image_store, _dir) = create_service_with_repo(30).await;
    let image = test_canonical_image(200);

    insert_legacy_row(
        &repository,
        &image_store,
        "rerun-item",
        image.png_bytes(),
        "d111111111111111111111111111111111111111111111111111111111111111",
        "2026-10-01T12:00:00Z",
        None,
    )
    .await;

    let summary1 = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary1.updated_in_place, 1);

    let summary2 = service.migrate_legacy_image_identities().await.unwrap();
    assert_eq!(summary2.candidates_found, 0);
    assert_eq!(summary2.updated_in_place, 0);
    assert_eq!(summary2.consolidated, 0);
    assert_eq!(summary2.skipped_corrupt_or_missing, 0);
    assert_eq!(summary2.skipped_stale, 0);
}
