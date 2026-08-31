use std::sync::Arc;

use history::ClipboardHistoryService;
use pookie_clipboard::ClipboardBackend;
use tokio::sync::Mutex;

use crate::clipboard_service::ClipboardService;
use crate::paste_backend::{PasteBackend, PasteCapability};

#[derive(Debug, PartialEq, Eq)]
pub enum ActivationResult {
    Pasted,
    ClipboardUpdated,
    PasteFailed,
    NotFound,
    UnsupportedContent,
}

pub struct ClipboardActivationService<B, P>
where
    B: ClipboardBackend,
    P: PasteBackend,
{
    history_service: Arc<ClipboardHistoryService>,
    clipboard_service: Arc<Mutex<ClipboardService<B>>>,
    paste_backend: P,
}

impl<B, P> ClipboardActivationService<B, P>
where
    B: ClipboardBackend,
    P: PasteBackend,
{
    pub fn new(
        history_service: Arc<ClipboardHistoryService>,
        clipboard_service: Arc<Mutex<ClipboardService<B>>>,
        paste_backend: P,
    ) -> Self {
        Self {
            history_service,
            clipboard_service,
            paste_backend,
        }
    }

    pub async fn activate_to_clipboard(&self, id: &str) -> anyhow::Result<ActivationResult> {
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

        match self.paste_backend.capability() {
            PasteCapability::Direct => match self.paste_backend.paste() {
                Ok(()) => Ok(ActivationResult::Pasted),

                Err(_) => Ok(ActivationResult::PasteFailed),
            },

            PasteCapability::ClipboardOnly => Ok(ActivationResult::ClipboardUpdated),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};

    use history::{ClipboardHistoryService, HistoryConfig};
    use pookie_clipboard::{ClipboardBackend, ClipboardContent, ClipboardError};
    use pookie_core::ClipboardItem;
    use storage::{Database, StorageRepository};
    use tokio::sync::Mutex;

    use super::{ActivationResult, ClipboardActivationService};
    use crate::clipboard_service::ClipboardService;

    use std::sync::atomic::{AtomicBool, Ordering};

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
    ) -> ClipboardActivationService<FakeClipboardBackend, FakePasteBackend> {
        let backend = FakeClipboardBackend { written };

        let clipboard_service = StdArc::new(Mutex::new(ClipboardService::new(backend)));

        ClipboardActivationService::new(history_service, clipboard_service, paste_backend)
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

        assert!(!pasted.load(Ordering::SeqCst));

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
            .activate_to_clipboard(&b_id)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::Pasted,);

        assert!(pasted.load(Ordering::SeqCst));

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned")
                .as_deref(),
            Some("B")
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items.len(), 3);

        assert_eq!(items[0].id, b_id);

        assert_eq!(items[0].text_content.as_deref(), Some("B"));

        assert_eq!(items[1].id, c_id);

        assert_eq!(items[2].id, a_id);
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
            .activate_to_clipboard("missing")
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::NotFound);

        assert!(!pasted.load(Ordering::SeqCst));

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned")
                .as_deref(),
            None
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
            .activate_to_clipboard(&b_id)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::ClipboardUpdated,);

        assert!(!pasted.load(Ordering::SeqCst));

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned")
                .as_deref(),
            Some("B")
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items[0].id, b_id);

        assert_eq!(items[0].text_content.as_deref(), Some("B"));
    }

    #[tokio::test]
    async fn paste_failure_returns_partial_failure_after_clipboard_update_and_promotion() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let backend = FakeClipboardBackend {
            written: StdArc::clone(&written),
        };

        let clipboard_service = StdArc::new(Mutex::new(ClipboardService::new(backend)));

        let activation_service = ClipboardActivationService::new(
            StdArc::clone(&history_service),
            clipboard_service,
            FailingPasteBackend,
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
            .activate_to_clipboard(&b_id)
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::PasteFailed,);

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned")
                .as_deref(),
            Some("B")
        );

        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items[0].id, b_id);

        assert_eq!(items[0].text_content.as_deref(), Some("B"));
    }
}
