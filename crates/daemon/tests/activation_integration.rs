use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};

use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;

use daemon::{
    activation_service::{ActivationResult, ClipboardActivationService},
    clipboard_service::ClipboardService,
    focus_backend::{FocusBackend, FocusError, FocusTarget, UnavailableFocusBackend},
    focus_service::FocusService,
    paste_backend::{ClipboardOnlyPasteBackend, PasteBackend, PasteCapability, PasteError},
};

use history::{ClipboardHistoryService, HistoryConfig};

use pookie_clipboard::{ClipboardBackend, ClipboardContent, ClipboardError};

use pookie_core::ClipboardItem;

use storage::{Database, StorageRepository};

use uuid::Uuid;

#[derive(Clone)]
struct FakeClipboardBackend {
    content: Arc<StdMutex<String>>,
}

impl FakeClipboardBackend {
    fn new(initial: &str) -> Self {
        Self {
            content: Arc::new(StdMutex::new(initial.to_string())),
        }
    }

    fn content(&self) -> String {
        self.content
            .lock()
            .expect("fake clipboard mutex poisoned")
            .clone()
    }

    fn set_external(&self, content: &str) {
        *self.content.lock().expect("fake clipboard mutex poisoned") = content.to_string();
    }
}

impl ClipboardBackend for FakeClipboardBackend {
    fn read(&self) -> Result<String, ClipboardError> {
        Ok(self
            .content
            .lock()
            .expect("fake clipboard mutex poisoned")
            .clone())
    }

    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        *self.content.lock().expect("fake clipboard mutex poisoned") = content.to_string();

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

/*
 * These integration tests are not testing
 * real X11 focus restoration.
 *
 * This backend immediately reports focus
 * restoration as successful.
 */
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

struct FailingFocusBackend;

impl FocusBackend for FailingFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        Ok(FocusTarget::new(42))
    }

    fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
        Err(FocusError::Failed("restore failed".to_string()))
    }

    fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
        Ok(false)
    }
}

fn test_item(text: &str, hash: &str, created_at: DateTime<Utc>) -> ClipboardItem {
    ClipboardItem {
        id: Uuid::new_v4(),
        content: ClipboardContent::Text(text.to_string()),
        hash: hash.to_string(),
        created_at,
    }
}

async fn create_history_service() -> Arc<ClipboardHistoryService> {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let repository = StorageRepository::new(&database);

    Arc::new(ClipboardHistoryService::new(
        repository,
        HistoryConfig { max_items: 30 },
    ))
}

type TestActivationService =
    ClipboardActivationService<FakeClipboardBackend, FakePasteBackend, ImmediateFocusBackend>;

type TestClipboardService = Arc<Mutex<ClipboardService<FakeClipboardBackend>>>;

type ActivationStack = (
    TestActivationService,
    TestClipboardService,
    FakeClipboardBackend,
);

fn create_activation_stack(
    history_service: Arc<ClipboardHistoryService>,
    initial_clipboard: &str,
    pasted: Arc<AtomicBool>,
) -> ActivationStack {
    let backend = FakeClipboardBackend::new(initial_clipboard);

    let backend_handle = backend.clone();

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend)));

    let paste_backend = FakePasteBackend::direct(pasted);

    let focus_service = FocusService::new(ImmediateFocusBackend);

    let activation_service = ClipboardActivationService::new(
        history_service,
        Arc::clone(&clipboard_service),
        paste_backend,
        focus_service,
    );

    (activation_service, clipboard_service, backend_handle)
}

#[tokio::test]
async fn activation_writes_clipboard_promotes_item_and_suppresses_self_write() {
    let history_service = create_history_service().await;

    /*
        Keep the original history safely
        in the past because promote()
        uses Utc::now().

        Initial order:
        C
        B
        A
    */
    let base_time = Utc::now() - Duration::seconds(10);

    let a = test_item("A", "hash-a", base_time);

    let b = test_item("B", "hash-b", base_time + Duration::seconds(1));

    let c = test_item("C", "hash-c", base_time + Duration::seconds(2));

    let a_id = a.id.to_string();

    let b_id = b.id.to_string();

    let c_id = c.id.to_string();

    history_service.save(a).await.expect("save A failed");

    history_service.save(b).await.expect("save B failed");

    history_service.save(c).await.expect("save C failed");

    let pasted = Arc::new(AtomicBool::new(false));

    let (activation_service, clipboard_service, backend_handle) =
        create_activation_stack(Arc::clone(&history_service), "", Arc::clone(&pasted));

    /*
        Activate B.

        No focus target is supplied here
        because this test is specifically
        testing clipboard/history behavior.
    */
    let result = activation_service
        .activate(&b_id, None)
        .await
        .expect("activation failed");

    assert_eq!(result, ActivationResult::Pasted,);

    /*
        The direct paste backend should
        have been triggered.
    */
    assert!(
        pasted.load(Ordering::SeqCst,),
        "direct paste backend was not triggered"
    );

    /*
        Clipboard should now contain B.
    */
    assert_eq!(backend_handle.content(), "B",);

    /*
        B should have moved to newest,
        while preserving its ID.

        Expected:
        B
        C
        A
    */
    let items = history_service
        .get_all()
        .await
        .expect("history retrieval failed");

    assert_eq!(items.len(), 3,);

    assert_eq!(items[0].id, b_id,);

    assert_eq!(items[0].text_content.as_deref(), Some("B"),);

    assert_eq!(items[1].id, c_id,);

    assert_eq!(items[1].text_content.as_deref(), Some("C"),);

    assert_eq!(items[2].id, a_id,);

    assert_eq!(items[2].text_content.as_deref(), Some("A"),);

    /*
        Pookie wrote B itself.

        The monitoring side should NOT
        emit B again as a new change.
    */
    let change = {
        let mut clipboard = clipboard_service.lock().await;

        clipboard.check_for_change().expect("clipboard read failed")
    };

    assert!(
        change.is_none(),
        "self-written clipboard content was incorrectly detected as an external change"
    );

    /*
        Now simulate a genuine external
        application copying D.
    */
    backend_handle.set_external("D");

    let change = {
        let mut clipboard = clipboard_service.lock().await;

        clipboard.check_for_change().expect("clipboard read failed")
    };

    assert_eq!(change.as_deref(), Some("D"),);
}

#[tokio::test]
async fn missing_activation_does_not_change_clipboard() {
    let history_service = create_history_service().await;

    let pasted = Arc::new(AtomicBool::new(false));

    let (activation_service, clipboard_service, backend_handle) =
        create_activation_stack(Arc::clone(&history_service), "", Arc::clone(&pasted));

    let result = activation_service
        .activate("missing", None)
        .await
        .expect("activation failed");

    assert_eq!(result, ActivationResult::NotFound,);

    /*
        Missing activation must not
        trigger direct paste.
    */
    assert!(
        !pasted.load(Ordering::SeqCst,),
        "paste backend was triggered for a missing item"
    );

    /*
        Missing activation must not
        touch the OS clipboard.
    */
    assert_eq!(backend_handle.content(), "",);

    /*
        History should also remain empty.
    */
    let items = history_service
        .get_all()
        .await
        .expect("history retrieval failed");

    assert!(items.is_empty(),);

    /*
        Keep the complete activation stack
        alive for the duration of the test.
    */
    let _ = clipboard_service;
}

#[tokio::test]
async fn clipboard_only_activation_succeeds_without_focus_target() {
    let history_service = create_history_service().await;

    /*
        Keep the initial history in the past
        because promote() uses Utc::now().

        Initial order:
        C
        B
        A
    */
    let base_time = Utc::now() - Duration::seconds(10);

    let a = test_item("A", "wayland-hash-a", base_time);

    let b = test_item(
        "Wayland fallback item",
        "wayland-hash-b",
        base_time + Duration::seconds(1),
    );

    let c = test_item("C", "wayland-hash-c", base_time + Duration::seconds(2));

    let a_id = a.id.to_string();
    let b_id = b.id.to_string();
    let c_id = c.id.to_string();

    history_service.save(a).await.expect("save A failed");
    history_service.save(b).await.expect("save B failed");
    history_service.save(c).await.expect("save C failed");

    let backend = FakeClipboardBackend::new("");

    let backend_handle = backend.clone();

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend)));

    /*
        This represents the Wayland fallback:

        - no focus target support
        - no direct paste support
        - clipboard functionality remains available
    */
    let focus_service = FocusService::new(UnavailableFocusBackend);

    let activation_service = ClipboardActivationService::new(
        Arc::clone(&history_service),
        Arc::clone(&clipboard_service),
        ClipboardOnlyPasteBackend,
        focus_service,
    );

    /*
        No target is supplied.

        Therefore focus restoration must
        not be required for activation.
    */
    let result = activation_service
        .activate(&b_id, None)
        .await
        .expect("Wayland fallback activation failed");

    assert_eq!(result, ActivationResult::ClipboardUpdated,);

    /*
        Even without direct paste support,
        the selected content must still be
        written to the clipboard.
    */
    assert_eq!(backend_handle.content(), "Wayland fallback item",);

    /*
        Successful clipboard activation
        must still promote B.

        Expected:
        B
        C
        A
    */
    let items = history_service
        .get_all()
        .await
        .expect("history retrieval failed");

    assert_eq!(items.len(), 3);

    assert_eq!(items[0].id, b_id);
    assert_eq!(
        items[0].text_content.as_deref(),
        Some("Wayland fallback item"),
    );

    assert_eq!(items[1].id, c_id);
    assert_eq!(items[1].text_content.as_deref(), Some("C"),);

    assert_eq!(items[2].id, a_id);
    assert_eq!(items[2].text_content.as_deref(), Some("A"),);
}

#[tokio::test]
async fn x11_style_focus_safe_activation_restores_focus_and_pastes() {
    let history_service = create_history_service().await;

    let base_time = Utc::now() - Duration::seconds(10);

    let a = test_item("A", "x11-hash-a", base_time);

    let b = test_item(
        "X11 focused item",
        "x11-hash-b",
        base_time + Duration::seconds(1),
    );

    let c = test_item("C", "x11-hash-c", base_time + Duration::seconds(2));

    let a_id = a.id.to_string();
    let b_id = b.id.to_string();
    let c_id = c.id.to_string();

    history_service.save(a).await.expect("save A failed");
    history_service.save(b).await.expect("save B failed");
    history_service.save(c).await.expect("save C failed");

    let backend = FakeClipboardBackend::new("");
    let backend_handle = backend.clone();

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend)));

    let pasted = Arc::new(AtomicBool::new(false));

    let paste_backend = FakePasteBackend::direct(Arc::clone(&pasted));

    let focus_service = FocusService::new(ImmediateFocusBackend);

    let activation_service = ClipboardActivationService::new(
        Arc::clone(&history_service),
        clipboard_service,
        paste_backend,
        focus_service,
    );

    let result = activation_service
        .activate(&b_id, Some(FocusTarget::new(42)))
        .await
        .expect("X11-style activation failed");

    assert_eq!(result, ActivationResult::Pasted);

    assert_eq!(backend_handle.content(), "X11 focused item",);

    assert!(
        pasted.load(Ordering::SeqCst),
        "direct paste backend should have been called",
    );

    let items = history_service
        .get_all()
        .await
        .expect("history retrieval failed");

    assert_eq!(items.len(), 3);

    assert_eq!(items[0].id, b_id);
    assert_eq!(items[0].text_content.as_deref(), Some("X11 focused item"),);

    assert_eq!(items[1].id, c_id);
    assert_eq!(items[2].id, a_id);
}

#[tokio::test]
async fn focus_failure_prevents_direct_paste_but_keeps_clipboard_and_promotion() {
    let history_service = create_history_service().await;

    let base_time = Utc::now() - Duration::seconds(10);

    let a = test_item("A", "focus-failure-a", base_time);

    let b = test_item(
        "Focus failure item",
        "focus-failure-b",
        base_time + Duration::seconds(1),
    );

    let c = test_item("C", "focus-failure-c", base_time + Duration::seconds(2));

    let a_id = a.id.to_string();
    let b_id = b.id.to_string();
    let c_id = c.id.to_string();

    history_service.save(a).await.expect("save A failed");
    history_service.save(b).await.expect("save B failed");
    history_service.save(c).await.expect("save C failed");

    let backend = FakeClipboardBackend::new("");
    let backend_handle = backend.clone();

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend)));

    let pasted = Arc::new(AtomicBool::new(false));

    let paste_backend = FakePasteBackend::direct(Arc::clone(&pasted));

    let focus_service = FocusService::new(FailingFocusBackend);

    let activation_service = ClipboardActivationService::new(
        Arc::clone(&history_service),
        clipboard_service,
        paste_backend,
        focus_service,
    );

    let result = activation_service
        .activate(&b_id, Some(FocusTarget::new(42)))
        .await
        .expect("activation should return an application outcome");

    assert_eq!(result, ActivationResult::PasteFailed,);

    assert_eq!(
        backend_handle.content(),
        "Focus failure item",
        "clipboard should already be updated",
    );

    assert!(
        !pasted.load(Ordering::SeqCst),
        "paste must not execute after focus restoration failure",
    );

    let items = history_service
        .get_all()
        .await
        .expect("history retrieval failed");

    assert_eq!(items.len(), 3);

    assert_eq!(items[0].id, b_id, "item should still be promoted",);

    assert_eq!(items[0].text_content.as_deref(), Some("Focus failure item"),);

    assert_eq!(items[1].id, c_id);
    assert_eq!(items[2].id, a_id);
}
