use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};

use daemon::activation_service::{ActivationResult, ClipboardActivationService};
use daemon::clipboard_service::ClipboardService;
use daemon::clipboard_state::ClipboardState;
use daemon::focus_backend::{FocusBackend, FocusError, FocusTarget};
use daemon::focus_service::FocusService;
use daemon::paste_backend::{PasteBackend, PasteCapability, PasteError};

use history::{ClipboardHistoryService, HistoryConfig};

use pookie_clipboard::{ClipboardBackend, ClipboardContent, ClipboardError, canonicalize_rgba};

use pookie_core::ClipboardItem;

use storage::{Database, ImageStore, StorageRepository};

use tokio::sync::Mutex;

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
            "pookie-image-activation-test-{}-{timestamp}-{counter}",
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

#[derive(Clone)]
struct FakeClipboardBackend {
    written: Arc<StdMutex<Option<ClipboardContent>>>,
}

impl FakeClipboardBackend {
    fn new() -> Self {
        Self {
            written: Arc::new(StdMutex::new(None)),
        }
    }

    fn written(&self) -> Option<ClipboardContent> {
        self.written
            .lock()
            .expect("fake clipboard mutex poisoned")
            .clone()
    }
}

impl ClipboardBackend for FakeClipboardBackend {
    fn read(&self) -> Result<String, ClipboardError> {
        Ok(String::new())
    }

    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        self.write_content(&ClipboardContent::Text(content.to_string()))
    }

    fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
        *self.written.lock().expect("fake clipboard mutex poisoned") = Some(content.clone());

        Ok(())
    }
}

struct FakePasteBackend {
    capability: PasteCapability,

    pasted: Arc<AtomicBool>,
}

impl FakePasteBackend {
    fn direct(pasted: Arc<AtomicBool>) -> Self {
        Self {
            capability: PasteCapability::Direct,

            pasted,
        }
    }

    fn clipboard_only(pasted: Arc<AtomicBool>) -> Self {
        Self {
            capability: PasteCapability::ClipboardOnly,

            pasted,
        }
    }
}

impl PasteBackend for FakePasteBackend {
    fn capability(&self) -> PasteCapability {
        self.capability
    }

    fn paste(&self) -> Result<(), PasteError> {
        self.pasted.store(true, Ordering::SeqCst);

        Ok(())
    }
}

struct ImmediateFocusBackend;

impl FocusBackend for ImmediateFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        Ok(FocusTarget::new(1))
    }

    fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
        Ok(())
    }

    fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
        Ok(true)
    }
}

async fn create_history_service() -> (Arc<ClipboardHistoryService>, ImageStore, TestDirectory) {
    let directory = TestDirectory::new();

    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    let image_store = ImageStore::new(&directory.path);

    let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 })
        .with_image_store(image_store.clone());

    (Arc::new(service), image_store, directory)
}

fn canonical_test_image() -> Vec<u8> {
    /*
     * 2×2:
     *
     * red   green
     * blue  white
     */
    canonicalize_rgba(
        2,
        2,
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
    )
    .expect("failed creating canonical test PNG")
}

fn image_item(
    id: uuid::Uuid,
    image: Vec<u8>,
    created_at: chrono::DateTime<chrono::Utc>,
) -> ClipboardItem {
    ClipboardItem {
        id,

        content: ClipboardContent::Image(image),

        hash: format!("image-{id}"),

        created_at,
    }
}

fn text_item(text: &str, created_at: chrono::DateTime<chrono::Utc>) -> ClipboardItem {
    let id = uuid::Uuid::new_v4();

    ClipboardItem {
        id,

        content: ClipboardContent::Text(text.to_string()),

        hash: format!("text-{id}"),

        created_at,
    }
}

fn activation_service(
    history_service: Arc<ClipboardHistoryService>,
    backend: FakeClipboardBackend,
    clipboard_state: Arc<ClipboardState>,
    paste_backend: FakePasteBackend,
) -> ClipboardActivationService<FakeClipboardBackend, FakePasteBackend, ImmediateFocusBackend> {
    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend, clipboard_state)));

    ClipboardActivationService::new(
        history_service,
        clipboard_service,
        paste_backend,
        FocusService::new(ImmediateFocusBackend),
    )
}

#[tokio::test]
async fn activates_persisted_image_writes_clipboard_pastes_and_promotes() {
    let (history_service, _image_store, _directory) = create_history_service().await;

    let canonical_png = canonical_test_image();

    let base_time = chrono::Utc::now() - chrono::Duration::seconds(10);

    let image_id = uuid::Uuid::new_v4();

    history_service
        .save(image_item(image_id, canonical_png.clone(), base_time))
        .await
        .expect("image save failed");

    history_service
        .save(text_item(
            "newer text item",
            base_time + chrono::Duration::seconds(1),
        ))
        .await
        .expect("text save failed");

    let backend = FakeClipboardBackend::new();

    let backend_handle = backend.clone();

    let clipboard_state = Arc::new(ClipboardState::default());

    let pasted = Arc::new(AtomicBool::new(false));

    let service = activation_service(
        Arc::clone(&history_service),
        backend,
        Arc::clone(&clipboard_state),
        FakePasteBackend::direct(Arc::clone(&pasted)),
    );

    let result = service
        .activate(&image_id.to_string(), None)
        .await
        .expect("image activation failed");

    assert_eq!(result, ActivationResult::Pasted,);

    assert!(pasted.load(Ordering::SeqCst,), "direct paste should run",);

    assert_eq!(
        backend_handle.written(),
        Some(ClipboardContent::Image(canonical_png.clone(),),),
    );

    /*
     * 10.7 integration:
     *
     * writeback must mark the image as self-generated so
     * the watcher does not create another history row.
     */
    assert!(clipboard_state.is_self_write(&ClipboardContent::Image(canonical_png,),),);

    let history = history_service
        .get_all()
        .await
        .expect("history retrieval failed");

    assert_eq!(
        history[0].id,
        image_id.to_string(),
        "activated image should become most recent",
    );
}

#[tokio::test]
async fn clipboard_only_image_activation_updates_clipboard_without_direct_paste() {
    let (history_service, _image_store, _directory) = create_history_service().await;

    let canonical_png = canonical_test_image();

    let image_id = uuid::Uuid::new_v4();

    history_service
        .save(image_item(
            image_id,
            canonical_png.clone(),
            chrono::Utc::now(),
        ))
        .await
        .expect("image save failed");

    let backend = FakeClipboardBackend::new();

    let backend_handle = backend.clone();

    let pasted = Arc::new(AtomicBool::new(false));

    let service = activation_service(
        history_service,
        backend,
        Arc::new(ClipboardState::default()),
        FakePasteBackend::clipboard_only(Arc::clone(&pasted)),
    );

    let result = service
        .activate(&image_id.to_string(), None)
        .await
        .expect("image activation failed");

    assert_eq!(result, ActivationResult::ClipboardUpdated,);

    assert!(!pasted.load(Ordering::SeqCst,),);

    assert_eq!(
        backend_handle.written(),
        Some(ClipboardContent::Image(canonical_png,),),
    );
}

#[tokio::test]
async fn missing_image_file_fails_without_clipboard_write_or_paste() {
    let (history_service, image_store, _directory) = create_history_service().await;

    let image_id = uuid::Uuid::new_v4();

    history_service
        .save(image_item(
            image_id,
            canonical_test_image(),
            chrono::Utc::now(),
        ))
        .await
        .expect("image save failed");

    let stored = history_service
        .get_by_id(&image_id.to_string())
        .await
        .expect("history lookup failed")
        .expect("image row missing");

    let file_path = stored.file_path.expect("image path missing");

    image_store
        .delete_image(&file_path)
        .await
        .expect("failed removing image file");

    let backend = FakeClipboardBackend::new();

    let backend_handle = backend.clone();

    let pasted = Arc::new(AtomicBool::new(false));

    let service = activation_service(
        history_service,
        backend,
        Arc::new(ClipboardState::default()),
        FakePasteBackend::direct(Arc::clone(&pasted)),
    );

    let result = service.activate(&image_id.to_string(), None).await;

    assert!(result.is_err(), "missing image should fail safely",);

    assert_eq!(backend_handle.written(), None,);

    assert!(!pasted.load(Ordering::SeqCst,),);
}

#[tokio::test]
async fn corrupt_image_file_fails_before_clipboard_write_or_paste() {
    let (history_service, image_store, _directory) = create_history_service().await;

    let image_id = uuid::Uuid::new_v4();

    history_service
        .save(image_item(
            image_id,
            canonical_test_image(),
            chrono::Utc::now(),
        ))
        .await
        .expect("image save failed");

    let stored = history_service
        .get_by_id(&image_id.to_string())
        .await
        .expect("history lookup failed")
        .expect("image row missing");

    let relative_path = stored.file_path.expect("image path missing");

    let absolute_path = image_store
        .resolve_relative_path(&relative_path)
        .expect("failed resolving image path");

    fs::write(absolute_path, b"not a png").expect("failed corrupting image test file");

    let backend = FakeClipboardBackend::new();

    let backend_handle = backend.clone();

    let pasted = Arc::new(AtomicBool::new(false));

    let service = activation_service(
        history_service,
        backend,
        Arc::new(ClipboardState::default()),
        FakePasteBackend::direct(Arc::clone(&pasted)),
    );

    let result = service.activate(&image_id.to_string(), None).await;

    assert!(result.is_err(), "corrupt image should fail safely",);

    assert_eq!(backend_handle.written(), None,);

    assert!(!pasted.load(Ordering::SeqCst,),);
}
