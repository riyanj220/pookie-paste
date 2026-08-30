use history::ClipboardHistoryService;
use ipc::{IpcRequest, IpcResponse};

use crate::ipc_mapper::to_history_item;

pub async fn handle_request(
    request: IpcRequest,
    history_service: &ClipboardHistoryService<'_>,
) -> IpcResponse {
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
    use super::*;

    use chrono::{Duration, Utc};
    use history::HistoryConfig;
    use pookie_clipboard::ClipboardContent;
    use pookie_core::ClipboardItem;
    use storage::{Database, StorageRepository};

    #[tokio::test]
    async fn handles_ping_request() {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

        let response = handle_request(IpcRequest::Ping, &service).await;

        assert_eq!(response, IpcResponse::Pong,);
    }

    #[tokio::test]
    async fn returns_clipboard_history() {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

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

        let response = handle_request(IpcRequest::GetHistory, &service).await;

        match response {
            IpcResponse::History { items } => {
                assert_eq!(items.len(), 2);

                assert_eq!(items[0].text_content.as_deref(), Some("Second"),);

                assert_eq!(items[1].text_content.as_deref(), Some("First"),);
            }

            other => {
                panic!("unexpected response: {other:?}");
            }
        }
    }

    #[tokio::test]
    async fn deletes_existing_clipboard_history_item() {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

        let item_id = uuid::Uuid::new_v4();

        let item = ClipboardItem {
            id: item_id,
            content: ClipboardContent::Text("Delete me".to_string()),
            hash: "delete-me-hash".to_string(),
            created_at: Utc::now(),
        };

        service.save(item).await.expect("history save failed");

        let response = handle_request(
            IpcRequest::DeleteItem {
                id: item_id.to_string(),
            },
            &service,
        )
        .await;

        assert_eq!(response, IpcResponse::Deleted { deleted: true },);

        let items = service.get_all().await.expect("history retrieval failed");

        assert!(items.is_empty(), "deleted item should no longer exist");
    }

    #[tokio::test]
    async fn reports_false_when_deleting_missing_item() {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

        let response = handle_request(
            IpcRequest::DeleteItem {
                id: "does-not-exist".to_string(),
            },
            &service,
        )
        .await;

        assert_eq!(response, IpcResponse::Deleted { deleted: false },);
    }

    #[tokio::test]
    async fn clears_clipboard_history() {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

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

        let response = handle_request(IpcRequest::ClearHistory, &service).await;

        assert_eq!(response, IpcResponse::Cleared { count: 3 },);

        let items = service.get_all().await.expect("history retrieval failed");

        assert!(items.is_empty(), "history should be empty after clear");
    }

    #[tokio::test]
    async fn clearing_empty_history_returns_zero() {
        let database = Database::new("sqlite::memory:")
            .await
            .expect("database initialization failed");

        let repository = StorageRepository::new(&database);

        let service = ClipboardHistoryService::new(repository, HistoryConfig { max_items: 30 });

        let response = handle_request(IpcRequest::ClearHistory, &service).await;

        assert_eq!(response, IpcResponse::Cleared { count: 0 },);

        let items = service.get_all().await.expect("history retrieval failed");

        assert!(items.is_empty());
    }
}
