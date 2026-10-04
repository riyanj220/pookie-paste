use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Utc;
use history::{ClipboardHistoryService, HistoryConfig, HistorySaveOutcome};
use pookie_clipboard::{ClipboardContent, canonicalize_rgba};
use pookie_core::ClipboardItem;
use storage::{Database, ImageStore, StorageRepository};
use uuid::Uuid;

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
            "pookie-history-revision-test-{}-{timestamp}-{counter}",
            std::process::id(),
        ));

        fs::create_dir_all(&path).expect("failed creating test directory");

        Self { path }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o755));
        let _ = fs::set_permissions(self.path.join("images"), fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.path);
    }
}

async fn create_test_service() -> (ClipboardHistoryService, TestDirectory) {
    let directory = TestDirectory::new();
    let database = Database::new("sqlite::memory:")
        .await
        .expect("failed creating test database");
    let repository = StorageRepository::new(&database);
    let image_store = ImageStore::new(&directory.path);
    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 20 })
        .with_image_store(image_store);
    (service, directory)
}

fn create_text_item(text: &str) -> ClipboardItem {
    ClipboardItem {
        id: Uuid::new_v4(),
        content: ClipboardContent::Text(text.to_string()),
        hash: format!("hash-{}", text),
        created_at: Utc::now(),
    }
}

fn create_image_item(id: Uuid, r: u8, g: u8, b: u8) -> ClipboardItem {
    let image = canonicalize_rgba(1, 1, &[r, g, b, 255]).expect("canonical image creation failed");
    let hash = image.identity().to_versioned_string();
    ClipboardItem {
        id,
        content: ClipboardContent::Image(image),
        hash,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn initial_revision_is_one() {
    let (service, _temp) = create_test_service().await;
    assert_eq!(service.current_revision(), 1);
}

#[tokio::test]
async fn text_mutations_advance_revision() {
    let (service, _temp) = create_test_service().await;
    let initial_rev = service.current_revision();

    // 1. Insert new text
    let item1 = create_text_item("first copy");
    let id1 = item1.id.to_string();
    let outcome1 = service.save(item1).await.expect("save text failed");
    assert_eq!(outcome1, HistorySaveOutcome::Inserted { id: id1.clone() });
    let rev_after_save = service.current_revision();
    assert!(
        rev_after_save > initial_rev,
        "new text save must advance revision"
    );

    // 2. Duplicate text recopy (promotion)
    let item1_recopy = create_text_item("first copy");
    let recopy_id = item1_recopy.id.to_string();
    let outcome2 = service
        .save(item1_recopy)
        .await
        .expect("recopy save failed");
    assert_eq!(outcome2, HistorySaveOutcome::Promoted { id: id1.clone() });
    assert_ne!(recopy_id, id1, "candidate UUID is distinct and discarded");
    let rev_after_recopy = service.current_revision();
    assert!(
        rev_after_recopy > rev_after_save,
        "duplicate text recopy must advance revision"
    );

    // 3. Pin item
    let pin_result = service.pin(&id1).await.expect("pin failed");
    assert!(pin_result);
    let rev_after_pin = service.current_revision();
    assert!(
        rev_after_pin > rev_after_recopy,
        "pin must advance revision"
    );

    // Pinning again (no-op) must not advance revision
    let pin_again = service.pin(&id1).await.expect("second pin failed");
    assert!(!pin_again);
    assert_eq!(
        service.current_revision(),
        rev_after_pin,
        "no-op pin must not advance revision"
    );

    // 4. Toggle pin
    let toggled = service
        .toggle_pin(&id1)
        .await
        .expect("toggle pin failed")
        .expect("item not found");
    assert!(!toggled, "was pinned, should now be unpinned");
    let rev_after_toggle = service.current_revision();
    assert!(
        rev_after_toggle > rev_after_pin,
        "toggle pin must advance revision"
    );

    // 5. Delete item
    let deleted = service.delete(&id1).await.expect("delete failed");
    assert!(deleted);
    let rev_after_delete = service.current_revision();
    assert!(
        rev_after_delete > rev_after_toggle,
        "delete must advance revision"
    );

    // Deleting non-existent ID must not advance revision
    let delete_again = service.delete(&id1).await.expect("delete again failed");
    assert!(!delete_again);
    assert_eq!(
        service.current_revision(),
        rev_after_delete,
        "failed delete must not advance revision"
    );
}

#[tokio::test]
async fn image_mutations_advance_revision() {
    let (service, _temp) = create_test_service().await;
    let initial_rev = service.current_revision();

    // 1. Save canonical image
    let img_id = Uuid::new_v4();
    let item = create_image_item(img_id, 255, 0, 0);
    let outcome1 = service.save(item).await.expect("save image failed");
    assert_eq!(
        outcome1,
        HistorySaveOutcome::Inserted {
            id: img_id.to_string(),
        }
    );
    let rev1 = service.current_revision();
    assert!(rev1 > initial_rev, "image save must advance revision");

    // 2. Duplicate image recopy (promotion)
    let candidate_img_id = Uuid::new_v4();
    let duplicate = create_image_item(candidate_img_id, 255, 0, 0);
    let outcome2 = service
        .save(duplicate)
        .await
        .expect("save duplicate failed");
    assert_eq!(
        outcome2,
        HistorySaveOutcome::Promoted {
            id: img_id.to_string(),
        }
    );
    let rev2 = service.current_revision();
    assert!(rev2 > rev1, "duplicate image recopy must advance revision");
}

#[tokio::test]
async fn missing_file_image_repair_returns_inserted() {
    let (service, temp) = create_test_service().await;
    let img_id1 = Uuid::new_v4();
    let item1 = create_image_item(img_id1, 12, 34, 56);
    let outcome1 = service.save(item1).await.expect("save image 1 failed");
    assert_eq!(
        outcome1,
        HistorySaveOutcome::Inserted {
            id: img_id1.to_string()
        }
    );

    // Delete backing file from disk to simulate missing/orphaned image file
    let file_path = temp.path.join("images").join(format!("{img_id1}.png"));
    assert!(file_path.exists());
    std::fs::remove_file(&file_path).expect("failed deleting image file");

    // Recopy same image content with candidate ID 2
    let img_id2 = Uuid::new_v4();
    let item2 = create_image_item(img_id2, 12, 34, 56);
    let outcome2 = service.save(item2).await.expect("save image 2 failed");

    // Old entity was replaced/repaired, so outcome MUST be Inserted with img_id2, NOT Promoted
    assert_eq!(
        outcome2,
        HistorySaveOutcome::Inserted {
            id: img_id2.to_string()
        }
    );

    let items = service.get_all().await.expect("get_all failed");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, img_id2.to_string());
}

#[tokio::test]
async fn clear_advances_revision_only_when_items_exist() {
    let (service, _temp) = create_test_service().await;

    // Clear on empty database must not advance revision
    let cleared_empty = service.clear().await.expect("clear empty failed");
    assert_eq!(cleared_empty, 0);
    assert_eq!(service.current_revision(), 1);

    // Add items
    service
        .save(create_text_item("alpha"))
        .await
        .expect("save alpha");
    service
        .save(create_text_item("beta"))
        .await
        .expect("save beta");
    let rev_before_clear = service.current_revision();

    let count = service.clear().await.expect("clear failed");
    assert_eq!(count, 2);
    let rev_after_clear = service.current_revision();
    assert!(
        rev_after_clear > rev_before_clear,
        "clear with items must advance revision"
    );
}

#[tokio::test]
async fn consistent_snapshot_matches_authoritative_revision() {
    let (service, _temp) = create_test_service().await;

    service
        .save(create_text_item("one"))
        .await
        .expect("save one");
    service
        .save(create_text_item("two"))
        .await
        .expect("save two");

    let (items, snapshot_rev) = service
        .get_all_snapshot()
        .await
        .expect("get_all_snapshot failed");

    assert_eq!(items.len(), 2);
    assert_eq!(snapshot_rev, service.current_revision());
    assert_eq!(items[0].text_content.as_deref(), Some("two"));
    assert_eq!(items[1].text_content.as_deref(), Some("one"));
}

#[tokio::test]
async fn subscriber_wakes_on_service_mutation() {
    let (service, _temp) = create_test_service().await;
    let mut rx = service.subscribe_revision();
    assert_eq!(*rx.borrow_and_update(), 1);

    service
        .save(create_text_item("waker"))
        .await
        .expect("save waker");

    assert!(rx.changed().await.is_ok());
    let new_rev = *rx.borrow_and_update();
    assert_eq!(new_rev, 2);
    assert_eq!(new_rev, service.current_revision());
}

#[tokio::test]
async fn delete_db_success_with_cleanup_failure_advances_revision() {
    let (service, temp) = create_test_service().await;
    let img_id = Uuid::new_v4();
    let id_str = img_id.to_string();
    service
        .save(create_image_item(img_id, 100, 150, 200))
        .await
        .expect("save image");
    let rev_before_delete = service.current_revision();

    // Verify item is present in DB
    let item_before = service.get_by_id(&id_str).await.expect("get_by_id");
    assert!(item_before.is_some());

    // Make images directory read-only so image file unlinking fails
    let images_dir = temp.path.join("images");
    fs::set_permissions(&images_dir, fs::Permissions::from_mode(0o555))
        .expect("chmod 0555 on images dir");

    let delete_result = service.delete(&id_str).await;

    // Restore permissions so cleanup works and test directory can drop cleanly
    let _ = fs::set_permissions(&images_dir, fs::Permissions::from_mode(0o755));

    // DB deletion succeeded, best-effort cleanup failed with warning
    assert!(delete_result.expect("delete should succeed even if file cleanup fails"));
    let rev_after_delete = service.current_revision();
    assert!(
        rev_after_delete > rev_before_delete,
        "revision must advance on authoritative SQLite deletion even if file cleanup fails"
    );

    // Verify item is truly deleted in SQLite
    let item_after = service.get_by_id(&id_str).await.expect("get_by_id");
    assert!(item_after.is_none());
}

#[tokio::test]
async fn clear_db_success_with_cleanup_failure_advances_revision() {
    let (service, temp) = create_test_service().await;
    let img1 = Uuid::new_v4();
    let img2 = Uuid::new_v4();
    service
        .save(create_image_item(img1, 10, 20, 30))
        .await
        .expect("save img1");
    service
        .save(create_image_item(img2, 40, 50, 60))
        .await
        .expect("save img2");
    let rev_before_clear = service.current_revision();

    // Make images directory read-only so file unlinking fails
    let images_dir = temp.path.join("images");
    fs::set_permissions(&images_dir, fs::Permissions::from_mode(0o555))
        .expect("chmod 0555 on images dir");

    let count = service.clear().await;

    let _ = fs::set_permissions(&images_dir, fs::Permissions::from_mode(0o755));

    assert_eq!(count.expect("clear should succeed"), 2);
    let rev_after_clear = service.current_revision();
    assert!(
        rev_after_clear > rev_before_clear,
        "clear must advance revision when SQLite is cleared even if file cleanup fails"
    );

    // Authoritative SQLite state must be empty
    let items = service.get_all().await.expect("get_all");
    assert!(items.is_empty());
}

#[tokio::test]
async fn insert_succeeds_and_enforce_limit_cleanup_fails_advances_revision() {
    let directory = TestDirectory::new();
    let database = Database::new("sqlite::memory:")
        .await
        .expect("failed creating test database");
    let repository = StorageRepository::new(&database);
    let image_store = ImageStore::new(&directory.path);
    // Config with max_items = 2
    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 2 })
        .with_image_store(image_store);

    let img1 = Uuid::new_v4();
    let img2 = Uuid::new_v4();
    service
        .save(create_image_item(img1, 1, 2, 3))
        .await
        .expect("save img1");
    service
        .save(create_image_item(img2, 4, 5, 6))
        .await
        .expect("save img2");
    let rev_before_third = service.current_revision();

    // Make images directory read-only so pruning file cleanup fails
    let images_dir = directory.path.join("images");
    fs::set_permissions(&images_dir, fs::Permissions::from_mode(0o555))
        .expect("chmod 0555 on images dir");

    // Saving a third item will trigger enforce_limit which deletes img1 from DB and attempts file cleanup
    let text_item = create_text_item("third item causing eviction");
    let save_result = service.save(text_item).await;

    let _ = fs::set_permissions(&images_dir, fs::Permissions::from_mode(0o755));

    save_result.expect("save should succeed despite file cleanup failure in enforce_limit");
    let rev_after_third = service.current_revision();
    assert!(
        rev_after_third > rev_before_third,
        "revision must advance to reflect newly inserted item and pruned DB state"
    );

    // Verify DB state: total items == 2, img1 was pruned
    let items = service.get_all().await.expect("get_all");
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.id != img1.to_string()));
}

#[tokio::test]
async fn no_op_pin_unpin_and_toggle_do_not_advance_revision() {
    let (service, _temp) = create_test_service().await;
    let initial_rev = service.current_revision();

    // 1. Non-existent operations must not advance revision
    let pin_missing = service.pin("non-existent-id").await.expect("pin missing");
    assert!(!pin_missing);
    assert_eq!(service.current_revision(), initial_rev);

    let unpin_missing = service
        .unpin("non-existent-id")
        .await
        .expect("unpin missing");
    assert!(!unpin_missing);
    assert_eq!(service.current_revision(), initial_rev);

    let toggle_missing = service
        .toggle_pin("non-existent-id")
        .await
        .expect("toggle missing");
    assert_eq!(toggle_missing, None);
    assert_eq!(service.current_revision(), initial_rev);

    // 2. Add an unpinned item
    let item = create_text_item("pin-test");
    let id_str = item.id.to_string();
    service.save(item).await.expect("save");
    let rev_saved = service.current_revision();
    assert!(rev_saved > initial_rev);

    // Unpinning an item that is ALREADY unpinned is a no-op
    let unpin_already_unpinned = service
        .unpin(&id_str)
        .await
        .expect("unpin already unpinned");
    assert!(!unpin_already_unpinned);
    assert_eq!(
        service.current_revision(),
        rev_saved,
        "unpinning already unpinned item must not advance revision"
    );

    // Pinning the item mutates DB and advances revision
    let pinned = service.pin(&id_str).await.expect("pin");
    assert!(pinned);
    let rev_pinned = service.current_revision();
    assert!(rev_pinned > rev_saved);

    // Pinning an item that is ALREADY pinned is a no-op
    let pin_already_pinned = service.pin(&id_str).await.expect("pin already pinned");
    assert!(!pin_already_pinned);
    assert_eq!(
        service.current_revision(),
        rev_pinned,
        "pinning already pinned item must not advance revision"
    );

    // Toggle pin unpins the item and advances revision
    let toggled_unpin = service.toggle_pin(&id_str).await.expect("toggle unpin");
    assert_eq!(toggled_unpin, Some(false));
    let rev_unpinned = service.current_revision();
    assert!(rev_unpinned > rev_pinned);

    // Toggle pin pins the item again and advances revision
    let toggled_pin = service.toggle_pin(&id_str).await.expect("toggle pin");
    assert_eq!(toggled_pin, Some(true));
    let rev_pinned_again = service.current_revision();
    assert!(rev_pinned_again > rev_unpinned);
}
