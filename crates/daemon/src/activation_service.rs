use std::sync::Arc;

use history::ClipboardHistoryService;
use pookie_clipboard::ClipboardBackend;
use tokio::sync::Mutex;

use crate::clipboard_service::ClipboardService;

#[derive(Debug, PartialEq, Eq)]
pub enum ActivationResult {
    Activated,
    NotFound,
    UnsupportedContent,
}

pub struct ClipboardActivationService<B>
where
    B: ClipboardBackend,
{
    history_service: Arc<ClipboardHistoryService>,
    clipboard_service: Arc<Mutex<ClipboardService<B>>>,
}

impl<B> ClipboardActivationService<B>
where
    B: ClipboardBackend,
{
    pub fn new(
        history_service: Arc<ClipboardHistoryService>,
        clipboard_service: Arc<Mutex<ClipboardService<B>>>,
    ) -> Self {
        Self {
            history_service,
            clipboard_service,
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

        Ok(ActivationResult::Activated)
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
    ) -> ClipboardActivationService<FakeClipboardBackend> {
        let backend = FakeClipboardBackend { written };

        let clipboard_service = StdArc::new(Mutex::new(ClipboardService::new(backend)));

        ClipboardActivationService::new(history_service, clipboard_service)
    }

    #[tokio::test]
    async fn activates_text_item_and_promotes_it_to_most_recent() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let activation_service =
            create_activation_service(StdArc::clone(&history_service), StdArc::clone(&written));

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

        assert_eq!(result, ActivationResult::Activated);

        // B was written to the clipboard.
        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned")
                .as_deref(),
            Some("B")
        );

        // B should now be the newest history item.
        let items = history_service
            .get_all()
            .await
            .expect("history retrieval failed");

        assert_eq!(items.len(), 3);

        assert_eq!(items[0].id, b_id);
        assert_eq!(items[0].text_content.as_deref(), Some("B"));

        assert_eq!(items[1].id, c_id);
        assert_eq!(items[1].text_content.as_deref(), Some("C"));

        assert_eq!(items[2].id, a_id);
        assert_eq!(items[2].text_content.as_deref(), Some("A"));
    }

    #[tokio::test]
    async fn missing_item_does_not_write_clipboard() {
        let history_service = create_history_service().await;

        let written = StdArc::new(StdMutex::new(None));

        let activation_service =
            create_activation_service(history_service, StdArc::clone(&written));

        let result = activation_service
            .activate_to_clipboard("missing")
            .await
            .expect("activation failed");

        assert_eq!(result, ActivationResult::NotFound);

        assert_eq!(
            written
                .lock()
                .expect("fake clipboard mutex poisoned")
                .as_deref(),
            None
        );
    }
}
