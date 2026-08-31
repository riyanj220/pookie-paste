use history::ClipboardHistoryService;
use ipc::{ActivationOutcome, IpcRequest, IpcResponse};
use pookie_clipboard::ClipboardBackend;

use crate::activation_service::{ActivationResult, ClipboardActivationService};
use crate::ipc_mapper::to_history_item;
use crate::paste_backend::PasteBackend;

pub async fn handle_request<B, P>(
    request: IpcRequest,
    history_service: &ClipboardHistoryService,
    activation_service: &ClipboardActivationService<B, P>,
) -> IpcResponse
where
    B: ClipboardBackend,
    P: PasteBackend,
{
    match request {
        IpcRequest::Ping => IpcResponse::Pong,

        IpcRequest::GetHistory => match history_service.get_all().await {
            Ok(items) => {
                let items = items.into_iter().map(to_history_item).collect();

                IpcResponse::History { items }
            }

            Err(error) => IpcResponse::Error {
                message: format!("failed to load clipboard history: {error}"),
            },
        },

        IpcRequest::ActivateItem { id } => {
            match activation_service.activate_to_clipboard(&id).await {
                Ok(result) => {
                    let outcome = match result {
                        ActivationResult::Pasted => ActivationOutcome::Pasted,

                        ActivationResult::ClipboardUpdated => ActivationOutcome::ClipboardUpdated,

                        ActivationResult::PasteFailed => ActivationOutcome::PasteFailed,

                        ActivationResult::NotFound => ActivationOutcome::NotFound,

                        ActivationResult::UnsupportedContent => {
                            ActivationOutcome::UnsupportedContent
                        }
                    };

                    IpcResponse::Activated { outcome }
                }

                Err(error) => {
                    tracing::error!(
                        error = %error,
                        item_id = %id,
                        "clipboard activation failed"
                    );

                    IpcResponse::Error {
                        message: "clipboard activation failed".to_string(),
                    }
                }
            }
        }

        IpcRequest::DeleteItem { id } => match history_service.delete(&id).await {
            Ok(deleted) => IpcResponse::Deleted { deleted },

            Err(error) => IpcResponse::Error {
                message: format!("failed to delete clipboard history item: {error}"),
            },
        },

        IpcRequest::ClearHistory => match history_service.clear().await {
            Ok(count) => IpcResponse::Cleared { count },

            Err(error) => IpcResponse::Error {
                message: format!("failed to clear clipboard history: {error}"),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use chrono::{Duration, Utc};

    use history::HistoryConfig;

    use pookie_clipboard::{ClipboardBackend, ClipboardContent, ClipboardError};

    use pookie_core::ClipboardItem;

    use storage::{Database, StorageRepository};

    use tokio::sync::Mutex;

    use super::*;

    use crate::{clipboard_service::ClipboardService, paste_backend::ClipboardOnlyPasteBackend};

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

    fn create_activation_service(
        history_service: Arc<ClipboardHistoryService>,
    ) -> (
        ClipboardActivationService<FakeClipboardBackend, ClipboardOnlyPasteBackend>,
        FakeClipboardBackend,
    ) {
        let backend = FakeClipboardBackend::new("");

        let backend_handle = backend.clone();

        let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend)));

        let activation_service = ClipboardActivationService::new(
            history_service,
            clipboard_service,
            ClipboardOnlyPasteBackend,
        );

        (activation_service, backend_handle)
    }

    #[tokio::test]
    async fn handles_ping_request() {
        let service = create_history_service().await;

        let (activation_service, _backend_handle) = create_activation_service(Arc::clone(&service));

        let response =
            handle_request(IpcRequest::Ping, service.as_ref(), &activation_service).await;

        assert_eq!(response, IpcResponse::Pong,);
    }

    #[tokio::test]
    async fn returns_clipboard_history() {
        let service = create_history_service().await;

        let first = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("First".to_string()),
            hash: "first-hash".to_string(),
            created_at: Utc::now(),
        };

        let second = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("Second".to_string()),
            hash: "second-hash".to_string(),
            created_at: Utc::now() + Duration::seconds(1),
        };

        service.save(first).await.expect("first save failed");

        service.save(second).await.expect("second save failed");

        let (activation_service, _backend_handle) = create_activation_service(Arc::clone(&service));

        let response = handle_request(
            IpcRequest::GetHistory,
            service.as_ref(),
            &activation_service,
        )
        .await;

        match response {
            IpcResponse::History { items } => {
                assert_eq!(items.len(), 2,);

                assert_eq!(items[0].text_content.as_deref(), Some("Second"),);

                assert_eq!(items[1].text_content.as_deref(), Some("First"),);
            }

            other => {
                panic!("unexpected response: {other:?}");
            }
        }
    }

    #[tokio::test]
    async fn activates_existing_clipboard_history_item() {
        let service = create_history_service().await;

        let item_id = uuid::Uuid::new_v4();

        let item = ClipboardItem {
            id: item_id,
            content: ClipboardContent::Text("Paste me".to_string()),
            hash: "paste-me-hash".to_string(),
            created_at: Utc::now(),
        };

        service.save(item).await.expect("history save failed");

        let (activation_service, backend_handle) = create_activation_service(Arc::clone(&service));

        let response = handle_request(
            IpcRequest::ActivateItem {
                id: item_id.to_string(),
            },
            service.as_ref(),
            &activation_service,
        )
        .await;

        assert_eq!(
            response,
            IpcResponse::Activated {
                outcome: ActivationOutcome::ClipboardUpdated,
            },
        );

        assert_eq!(backend_handle.content(), "Paste me",);
    }

    #[tokio::test]
    async fn activating_missing_item_returns_not_found() {
        let service = create_history_service().await;

        let (activation_service, backend_handle) = create_activation_service(Arc::clone(&service));

        let response = handle_request(
            IpcRequest::ActivateItem {
                id: "does-not-exist".to_string(),
            },
            service.as_ref(),
            &activation_service,
        )
        .await;

        assert_eq!(
            response,
            IpcResponse::Activated {
                outcome: ActivationOutcome::NotFound,
            },
        );

        assert_eq!(backend_handle.content(), "",);
    }

    #[tokio::test]
    async fn deletes_existing_clipboard_history_item() {
        let service = create_history_service().await;

        let item_id = uuid::Uuid::new_v4();

        let item = ClipboardItem {
            id: item_id,
            content: ClipboardContent::Text("Delete me".to_string()),
            hash: "delete-me-hash".to_string(),
            created_at: Utc::now(),
        };

        service.save(item).await.expect("history save failed");

        let (activation_service, _backend_handle) = create_activation_service(Arc::clone(&service));

        let response = handle_request(
            IpcRequest::DeleteItem {
                id: item_id.to_string(),
            },
            service.as_ref(),
            &activation_service,
        )
        .await;

        assert_eq!(response, IpcResponse::Deleted { deleted: true },);

        let items = service.get_all().await.expect("history retrieval failed");

        assert!(items.is_empty(), "deleted item should no longer exist");
    }

    #[tokio::test]
    async fn reports_false_when_deleting_missing_item() {
        let service = create_history_service().await;

        let (activation_service, _backend_handle) = create_activation_service(Arc::clone(&service));

        let response = handle_request(
            IpcRequest::DeleteItem {
                id: "does-not-exist".to_string(),
            },
            service.as_ref(),
            &activation_service,
        )
        .await;

        assert_eq!(response, IpcResponse::Deleted { deleted: false },);
    }

    #[tokio::test]
    async fn clears_clipboard_history() {
        let service = create_history_service().await;

        let first = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("A".to_string()),
            hash: "hash-a".to_string(),
            created_at: Utc::now(),
        };

        let second = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("B".to_string()),
            hash: "hash-b".to_string(),
            created_at: Utc::now() + Duration::seconds(1),
        };

        let third = ClipboardItem {
            id: uuid::Uuid::new_v4(),
            content: ClipboardContent::Text("C".to_string()),
            hash: "hash-c".to_string(),
            created_at: Utc::now() + Duration::seconds(2),
        };

        service.save(first).await.expect("first save failed");

        service.save(second).await.expect("second save failed");

        service.save(third).await.expect("third save failed");

        let (activation_service, _backend_handle) = create_activation_service(Arc::clone(&service));

        let response = handle_request(
            IpcRequest::ClearHistory,
            service.as_ref(),
            &activation_service,
        )
        .await;

        assert_eq!(response, IpcResponse::Cleared { count: 3 },);

        let items = service.get_all().await.expect("history retrieval failed");

        assert!(items.is_empty(), "history should be empty after clear");
    }

    #[tokio::test]
    async fn clearing_empty_history_returns_zero() {
        let service = create_history_service().await;

        let (activation_service, _backend_handle) = create_activation_service(Arc::clone(&service));

        let response = handle_request(
            IpcRequest::ClearHistory,
            service.as_ref(),
            &activation_service,
        )
        .await;

        assert_eq!(response, IpcResponse::Cleared { count: 0 },);

        let items = service.get_all().await.expect("history retrieval failed");

        assert!(items.is_empty(),);
    }
}
