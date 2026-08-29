use pookie_core::ClipboardItem;
use storage::{StorageRepository, StoredClipboardItem};

use crate::config::HistoryConfig;
use crate::mapper::to_stored_item;

pub struct ClipboardHistoryService<'a> {
    repository: StorageRepository<'a>,

    config: HistoryConfig,
}

impl<'a> ClipboardHistoryService<'a> {
    pub fn new(repository: StorageRepository<'a>, config: HistoryConfig) -> Self {
        Self { repository, config }
    }

    pub async fn save(&self, item: ClipboardItem) -> Result<(), sqlx::Error> {
        let stored_item = to_stored_item(item);

        self.repository.insert(&stored_item).await?;

        self.enforce_limit().await?;

        Ok(())
    }

    async fn enforce_limit(&self) -> Result<(), sqlx::Error> {
        let count = self.repository.count().await?;

        let max_items = self.config.max_items as i64;

        if count <= max_items {
            return Ok(());
        }

        let excess = count - max_items;

        let oldest_items = self.repository.get_oldest(excess).await?;

        let ids = oldest_items.into_iter().map(|item| item.id).collect();

        self.repository.delete_by_ids(ids).await?;

        Ok(())
    }

    pub async fn get_all(&self) -> Result<Vec<StoredClipboardItem>, sqlx::Error> {
        self.repository.get_all().await
    }

    pub async fn delete(&self, id: &str) -> Result<bool, sqlx::Error> {
        self.repository.delete_by_id(id).await
    }

    pub async fn clear(&self) -> Result<u64, sqlx::Error> {
        self.repository.clear().await
    }
}
