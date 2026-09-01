use std::sync::Arc;

use history::ClipboardHistoryService;
use pookie_clipboard::ClipboardBackend;
use tokio::sync::Mutex;

use crate::clipboard_service::ClipboardService;
use crate::focus_backend::{FocusBackend, FocusError, FocusTarget};
use crate::focus_service::FocusService;
use crate::paste_backend::{PasteBackend, PasteCapability};

#[derive(Debug, PartialEq, Eq)]
pub enum ActivationResult {
    Pasted,
    ClipboardUpdated,
    PasteFailed,
    NotFound,
    UnsupportedContent,
}

pub struct ClipboardActivationService<B, P, F>
where
    B: ClipboardBackend,
    P: PasteBackend,
    F: FocusBackend,
{
    history_service: Arc<ClipboardHistoryService>,
    clipboard_service: Arc<Mutex<ClipboardService<B>>>,
    paste_backend: P,
    focus_service: FocusService<F>,
}

impl<B, P, F> ClipboardActivationService<B, P, F>
where
    B: ClipboardBackend,
    P: PasteBackend,
    F: FocusBackend,
{
    pub fn new(
        history_service: Arc<ClipboardHistoryService>,
        clipboard_service: Arc<Mutex<ClipboardService<B>>>,
        paste_backend: P,
        focus_service: FocusService<F>,
    ) -> Self {
        Self {
            history_service,
            clipboard_service,
            paste_backend,
            focus_service,
        }
    }

    pub async fn activate(
        &self,
        id: &str,
        target: Option<FocusTarget>,
    ) -> anyhow::Result<ActivationResult> {
        let item = match self.history_service.get_by_id(id).await? {
            Some(item) => item,

            None => {
                return Ok(ActivationResult::NotFound);
            }
        };

        if item.content_type != "text" {
            return Ok(ActivationResult::UnsupportedContent);
        }

        let text = match item.text_content {
            Some(text) => text,

            None => {
                return Ok(ActivationResult::UnsupportedContent);
            }
        };

        {
            let mut clipboard = self.clipboard_service.lock().await;

            clipboard.write(&text)?;
        }

        let promoted = self.history_service.promote(id).await?;

        if !promoted {
            return Ok(ActivationResult::NotFound);
        }

        if let Some(target) = target
            && let Err(error) = self.focus_service.restore_and_wait(target).await
        {
            tracing::error!(
                error = ?error,
                target_id = target.id(),
                "focus restoration failed"
            );

            return Ok(ActivationResult::PasteFailed);
        }

        match self.paste_backend.capability() {
            PasteCapability::Direct => match self.paste_backend.paste() {
                Ok(()) => Ok(ActivationResult::Pasted),

                Err(_) => Ok(ActivationResult::PasteFailed),
            },

            PasteCapability::ClipboardOnly => Ok(ActivationResult::ClipboardUpdated),
        }
    }

    pub fn capture_target(&self) -> Result<FocusTarget, FocusError> {
        self.focus_service.capture_target()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc as StdArc, Mutex as StdMutex};

    use history::{ClipboardHistoryService, HistoryConfig};
    use pookie_clipboard::{ClipboardBackend, ClipboardContent, ClipboardError};
    use pookie_core::ClipboardItem;
    use storage::{Database, StorageRepository};
    use tokio::sync::Mutex;

    use super::{ActivationResult, ClipboardActivationService};
    use crate::clipboard_service::ClipboardService;
    use crate::focus_backend::{FocusBackend, FocusError, FocusTarget};
    use crate::focus_service::FocusService;
    use crate::paste_backend::{PasteBackend, PasteCapability, PasteError};

    struct FakeClipboardBackend {
        written: StdArc<StdMutex<Option<String>>>,
    }

    impl ClipboardBackend for FakeClipboardBackend {
        fn read(&self) -> Result<String, ClipboardError> {
            Ok(String::new())
        }

        fn write(&self, content: &str) -> Result<(), ClipboardError> {
            *self.written.lock().expect("fake clipboard mutex poisoned") =
                Some(content.to_string());

            Ok(())
        }
    }

    struct FakePasteBackend {
        capability: PasteCapability,
        pasted: StdArc<AtomicBool>,
    }

    impl FakePasteBackend {
        fn direct(pasted: StdArc<AtomicBool>) -> Self {
            Self {
                capability: PasteCapability::Direct,
                pasted,
            }
        }

        fn clipboard_only(pasted: StdArc<AtomicBool>) -> Self {
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

    struct FailingPasteBackend;

    impl PasteBackend for FailingPasteBackend {
        fn capability(&self) -> PasteCapability {
            PasteCapability::Direct
        }

        fn paste(&self) -> Result<(), PasteError> {
            Err(PasteError::Unavailable)
        }
    }

    /*
     * Focus backend used by normal activation tests.
     *
     * It immediately confirms that the requested
     * target became active.
     */
    struct ImmediateFocusBackend {
        restore_called: StdArc<AtomicBool>,
        active_checked: StdArc<AtomicBool>,
    }

    impl ImmediateFocusBackend {
        fn new() -> Self {
            Self {
                restore_called: StdArc::new(AtomicBool::new(false)),
                active_checked: StdArc::new(AtomicBool::new(false)),
            }
        }
    }

    impl FocusBackend for ImmediateFocusBackend {
        fn active_target(&self) -> Result<FocusTarget, FocusError> {
            Ok(FocusTarget::new(1))
        }

        fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
            self.restore_called.store(true, Ordering::SeqCst);

            Ok(())
        }

        fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
            self.active_checked.store(true, Ordering::SeqCst);

            Ok(true)
        }
    }

    /*
     * Used to verify that a focus restoration
     * failure prevents direct paste while leaving
     * clipboard writeback and history promotion
     * intact.
     */
    struct FailingFocusBackend {
        restore_called: StdArc<AtomicBool>,
        active_checked: StdArc<AtomicBool>,
    }

    impl FocusBackend for FailingFocusBackend {
        fn active_target(&self) -> Result<FocusTarget, FocusError> {
            Ok(FocusTarget::new(1))
        }

        fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
            self.restore_called.store(true, Ordering::SeqCst);

            Err(FocusError::Failed(
                "simulated focus restoration failure".to_string(),
            ))
        }

        fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
            self.active_checked.store(true, Ordering::SeqCst);

            Ok(false)
        }
    }

    async fn create_history_service() -> StdArc<ClipboardHistoryService> {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        StdArc::new(ClipboardHistoryService::new(
            repository,
            HistoryConfig { max_items: 30 },
        ))
    }

    fn create_activation_service(
        history_service: StdArc<ClipboardHistoryService>,
        written: StdArc<StdMutex<Option<String>>>,
        paste_backend: FakePasteBackend,
    ) -> ClipboardActivationService<FakeClipboardBackend, FakePasteBackend, ImmediateFocusBackend>
    {
        let backend = FakeClipboardBackend { written };

        let clipboard_service = StdArc::new(Mutex::new(ClipboardService::new(backend)));

        let focus_service = FocusService::new(ImmediateFocusBackend::new());

        ClipboardActivationService::new(
            history_service,
            clipboard_service,
            paste_backend,
            focus_service,
        )
    }

    #[tokio::test]
    async fn activates_text_item_and_promotes_it_to_most_recent() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let pasted = StdArc::new(AtomicBool::new(false));

        let paste_backend = FakePasteBackend::direct(StdArc::clone(&pasted));

        let activation_service = create_activation_service(
            StdArc::clone(&history_service),
            StdArc::clone(&written),
            paste_backend,
        );

        assert!(!pasted.load(Ordering::SeqCst,));

        let base_time = chrono::Utc::now() - chrono::Duration::seconds(10);

        let a = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("A".to_string()),
            hash: "activation-a".to_string(),
            created_at: base_time,
        };

        let b = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("B".to_string()),
            hash: "activation-b".to_string(),
            created_at: base_time + chrono::Duration::seconds(1),
        };

        let c = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("C".to_string()),
            hash: "activation-c".to_string(),
            created_at: base_time + chrono::Duration::seconds(2),
        };

        let a_id = a.id.to_string();
        let b_id = b.id.to_string();
        let c_id = c.id.to_string();

        history_service.save(a).await.expect("save A failed");

        history_service.save(b).await.expect("save B failed");

        history_service.save(c).await.expect("save C failed");

        let result = activation_service
            .activate(&b_id, None)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::Pasted,);

        assert!(pasted.load(Ordering::SeqCst,));

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned",)
                .as_deref(),
            Some("B")
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items.len(), 3,);

        assert_eq!(items[0].id, b_id,);

        assert_eq!(items[0].text_content.as_deref(), Some("B"),);

        assert_eq!(items[1].id, c_id,);

        assert_eq!(items[2].id, a_id,);
    }

    #[tokio::test]
    async fn missing_item_does_not_write_clipboard() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let pasted = StdArc::new(AtomicBool::new(false));

        let paste_backend = FakePasteBackend::direct(StdArc::clone(&pasted));

        let activation_service =
            create_activation_service(history_service, StdArc::clone(&written), paste_backend);

        let result = activation_service
            .activate("missing", None)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::NotFound,);

        assert!(!pasted.load(Ordering::SeqCst,));

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned",)
                .as_deref(),
            None,
        );
    }

    #[tokio::test]
    async fn clipboard_only_activation_updates_clipboard_without_pasting() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let pasted = StdArc::new(AtomicBool::new(false));

        let paste_backend = FakePasteBackend::clipboard_only(StdArc::clone(&pasted));

        let activation_service = create_activation_service(
            StdArc::clone(&history_service),
            StdArc::clone(&written),
            paste_backend,
        );

        let base_time = chrono::Utc::now() - chrono::Duration::seconds(10);

        let a = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("A".to_string()),
            hash: "clipboard-only-a".to_string(),
            created_at: base_time,
        };

        let b = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("B".to_string()),
            hash: "clipboard-only-b".to_string(),
            created_at: base_time + chrono::Duration::seconds(1),
        };

        let b_id = b.id.to_string();

        let c = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("C".to_string()),
            hash: "clipboard-only-c".to_string(),
            created_at: base_time + chrono::Duration::seconds(2),
        };

        history_service.save(a).await.expect("save A failed");

        history_service.save(b).await.expect("save B failed");

        history_service.save(c).await.expect("save C failed");

        let result = activation_service
            .activate(&b_id, None)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::ClipboardUpdated,);

        assert!(!pasted.load(Ordering::SeqCst,));

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned",)
                .as_deref(),
            Some("B"),
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items[0].id, b_id,);

        assert_eq!(items[0].text_content.as_deref(), Some("B"),);
    }

    #[tokio::test]
    async fn paste_failure_returns_partial_failure_after_clipboard_update_and_promotion() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let backend = FakeClipboardBackend {
            written: StdArc::clone(&written),
        };

        let clipboard_service = StdArc::new(Mutex::new(ClipboardService::new(backend)));

        let focus_service = FocusService::new(ImmediateFocusBackend::new());

        let activation_service = ClipboardActivationService::new(
            StdArc::clone(&history_service),
            clipboard_service,
            FailingPasteBackend,
            focus_service,
        );

        let base_time = chrono::Utc::now() - chrono::Duration::seconds(10);

        let a = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("A".to_string()),
            hash: "failure-a".to_string(),
            created_at: base_time,
        };

        let b = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("B".to_string()),
            hash: "failure-b".to_string(),
            created_at: base_time + chrono::Duration::seconds(1),
        };

        let b_id = b.id.to_string();

        let c = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("C".to_string()),
            hash: "failure-c".to_string(),
            created_at: base_time + chrono::Duration::seconds(2),
        };

        history_service.save(a).await.expect("save A failed");

        history_service.save(b).await.expect("save B failed");

        history_service.save(c).await.expect("save C failed");

        let result = activation_service
            .activate(&b_id, None)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::PasteFailed,);

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned",)
                .as_deref(),
            Some("B"),
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items[0].id, b_id,);

        assert_eq!(items[0].text_content.as_deref(), Some("B"),);
    }

    #[tokio::test]
    async fn focused_activation_restores_focus_confirms_active_and_pastes() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let pasted = StdArc::new(AtomicBool::new(false));

        let restore_called = StdArc::new(AtomicBool::new(false));

        let active_checked = StdArc::new(AtomicBool::new(false));

        let focus_backend = ImmediateFocusBackend {
            restore_called: StdArc::clone(&restore_called),
            active_checked: StdArc::clone(&active_checked),
        };

        let backend = FakeClipboardBackend {
            written: StdArc::clone(&written),
        };

        let clipboard_service = StdArc::new(Mutex::new(ClipboardService::new(backend)));

        let paste_backend = FakePasteBackend::direct(StdArc::clone(&pasted));

        let focus_service = FocusService::new(focus_backend);

        let activation_service = ClipboardActivationService::new(
            StdArc::clone(&history_service),
            clipboard_service,
            paste_backend,
            focus_service,
        );

        let item = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("Focused paste".to_string()),
            hash: "focused-activation".to_string(),
            created_at: chrono::Utc::now() - chrono::Duration::seconds(10),
        };

        let item_id = item.id.to_string();

        history_service
            .save(item)
            .await
            .expect("history save failed");

        let result = activation_service
            .activate(&item_id, Some(FocusTarget::new(12345)))
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::Pasted,);

        assert!(
            restore_called.load(Ordering::SeqCst,),
            "focus restore should be called",
        );

        assert!(
            active_checked.load(Ordering::SeqCst,),
            "focus should be confirmed before paste",
        );

        assert!(pasted.load(Ordering::SeqCst,), "paste should be triggered",);

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned",)
                .as_deref(),
            Some("Focused paste"),
        );
    }

    #[tokio::test]
    async fn focus_restoration_failure_keeps_clipboard_and_promotion_but_does_not_paste() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let pasted = StdArc::new(AtomicBool::new(false));

        let restore_called = StdArc::new(AtomicBool::new(false));

        let active_checked = StdArc::new(AtomicBool::new(false));

        let focus_backend = FailingFocusBackend {
            restore_called: StdArc::clone(&restore_called),
            active_checked: StdArc::clone(&active_checked),
        };

        let backend = FakeClipboardBackend {
            written: StdArc::clone(&written),
        };

        let clipboard_service = StdArc::new(Mutex::new(ClipboardService::new(backend)));

        let paste_backend = FakePasteBackend::direct(StdArc::clone(&pasted));

        let focus_service = FocusService::new(focus_backend);

        let activation_service = ClipboardActivationService::new(
            StdArc::clone(&history_service),
            clipboard_service,
            paste_backend,
            focus_service,
        );

        let base_time = chrono::Utc::now() - chrono::Duration::seconds(10);

        let a = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("A".to_string()),
            hash: "focus-failure-a".to_string(),
            created_at: base_time,
        };

        let b = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("B".to_string()),
            hash: "focus-failure-b".to_string(),
            created_at: base_time + chrono::Duration::seconds(1),
        };

        let c = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("C".to_string()),
            hash: "focus-failure-c".to_string(),
            created_at: base_time + chrono::Duration::seconds(2),
        };

        let b_id = b.id.to_string();

        history_service.save(a).await.expect("save A failed");

        history_service.save(b).await.expect("save B failed");

        history_service.save(c).await.expect("save C failed");

        let result = activation_service
            .activate(&b_id, Some(FocusTarget::new(12345)))
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::PasteFailed,);

        assert!(
            restore_called.load(Ordering::SeqCst,),
            "focus restoration should have been attempted",
        );

        assert!(
            !active_checked.load(Ordering::SeqCst,),
            "active-state polling should not happen when restore itself fails",
        );

        assert!(
            !pasted.load(Ordering::SeqCst,),
            "paste must not happen after focus restoration failure",
        );

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned",)
                .as_deref(),
            Some("B"),
            "clipboard should already be updated",
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items[0].id, b_id, "activated item should still be promoted",);

        assert_eq!(items[0].text_content.as_deref(), Some("B"),);
    }

    #[tokio::test]
    async fn unsupported_content_does_not_change_clipboard_or_trigger_paste() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let pasted = StdArc::new(AtomicBool::new(false));

        let paste_backend = FakePasteBackend::direct(StdArc::clone(&pasted));

        let activation_service = create_activation_service(
            StdArc::clone(&history_service),
            StdArc::clone(&written),
            paste_backend,
        );

        let item = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Image(vec![1, 2, 3, 4]),
            hash: "unsupported-image".to_string(),
            created_at: chrono::Utc::now(),
        };

        let item_id = item.id.to_string();

        history_service
            .save(item)
            .await
            .expect("history save failed");

        let result = activation_service
            .activate(&item_id, None)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::UnsupportedContent,);

        assert!(
            !pasted.load(Ordering::SeqCst),
            "paste backend must not be called for unsupported content",
        );

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned")
                .as_deref(),
            None,
            "clipboard must remain unchanged for unsupported content",
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items.len(), 1);

        assert_eq!(
            items[0].id, item_id,
            "unsupported item must not be promoted or replaced",
        );
    }
}
