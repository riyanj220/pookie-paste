use pookie_core::ClipboardItem;
use storage::StorageRepository;

use crate::mapper::to_stored_item;

pub struct ClipboardHistoryService<'a> {
    repository: StorageRepository<'a>,
}

impl<'a> ClipboardHistoryService<'a> {
    pub fn new(repository: StorageRepository<'a>) -> Self {
        Self { repository }
    }

    pub async fn save(&self, item: ClipboardItem) -> Result<(), sqlx::Error> {
        let stored_item = to_stored_item(item);

        self.repository.insert(&stored_item).await?;

        Ok(())
    }
}
