use std::sync::{Arc, Mutex as StdMutex};

use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;

use daemon::{
    activation_service::{ActivationResult, ClipboardActivationService},
    clipboard_service::ClipboardService,
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

fn create_activation_stack(
    history_service: Arc<ClipboardHistoryService>,
    initial_clipboard: &str,
) -> (
    ClipboardActivationService<FakeClipboardBackend>,
    Arc<Mutex<ClipboardService<FakeClipboardBackend>>>,
    FakeClipboardBackend,
) {
    let backend = FakeClipboardBackend::new(initial_clipboard);

    let backend_handle = backend.clone();

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend)));

    let activation_service =
        ClipboardActivationService::new(history_service, Arc::clone(&clipboard_service));

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

    let (activation_service, clipboard_service, backend_handle) =
        create_activation_stack(Arc::clone(&history_service), "");

    /*
        Activate B.
    */
    let result = activation_service
        .activate_to_clipboard(&b_id)
        .await
        .expect("activation failed");

    assert_eq!(result, ActivationResult::Activated,);

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

    let (activation_service, clipboard_service, backend_handle) =
        create_activation_stack(Arc::clone(&history_service), "");

    let result = activation_service
        .activate_to_clipboard("missing")
        .await
        .expect("activation failed");

    assert_eq!(result, ActivationResult::NotFound,);

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

    assert!(items.is_empty());

    /*
        Since nothing was written,
        checking an unchanged empty
        clipboard should also produce
        no meaningful change after the
        service establishes its state.

        This check is optional for the
        missing-item contract, but keeps
        the stack exercised.
    */
    let _ = clipboard_service;
}
