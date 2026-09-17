use std::collections::HashSet;

use pookie_clipboard::ClipboardContent;
use pookie_core::ClipboardItem;
use storage::{ImageStore, StorageRepository, StoredClipboardItem};

use crate::config::HistoryConfig;
use crate::error::HistoryError;
use crate::mapper::{to_stored_image_item, to_stored_text_item};

pub struct ClipboardHistoryService {
    repository: StorageRepository,

    config: HistoryConfig,

    image_store: Option<ImageStore>,
}

impl ClipboardHistoryService {
    pub fn new(repository: StorageRepository, config: HistoryConfig) -> Self {
        Self {
            repository,
            config,
            image_store: None,
        }
    }

    /// Attach filesystem-backed image persistence.
    ///
    /// Keeping this as a builder preserves the existing
    /// constructor used by text-only unit/integration tests
    /// while production configures the real image store.
    pub fn with_image_store(mut self, image_store: ImageStore) -> Self {
        self.image_store = Some(image_store);

        self
    }

    pub async fn save(&self, item: ClipboardItem) -> Result<(), HistoryError> {
        let ClipboardItem {
            id,
            content,
            hash,
            created_at,
        } = item;

        match content {
            ClipboardContent::Text(text) => self.save_text(id, text, hash, created_at).await,

            ClipboardContent::Image(image) => self.save_image(id, image, hash, created_at).await,
        }
    }

    async fn save_text(
        &self,
        id: uuid::Uuid,
        text: String,
        hash: String,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), HistoryError> {
        if let Some(existing) = self.repository.find_by_hash_and_type(&hash, "text").await? {
            self.repository.delete_by_id(&existing.id).await?;
        }

        let stored_item = to_stored_text_item(id, text, hash, created_at);

        self.repository.insert(&stored_item).await?;

        self.enforce_limit().await
    }

    async fn save_image(
        &self,
        id: uuid::Uuid,
        image: Vec<u8>,
        hash: String,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), HistoryError> {
        /*
         * A duplicate image already has exactly the same
         * canonical PNG bytes.
         *
         * Reuse its owned file and simply move the existing
         * history row to the most-recent position.
         */
        if let Some(existing) = self
            .repository
            .find_by_hash_and_type(&hash, "image")
            .await?
        {
            if let Some(image_store) = self.image_store.as_ref() {
                if let Some(file_path) = existing.file_path.as_deref()
                    && image_store.image_exists(file_path).await?
                {
                    self.repository
                        .update_created_at(&existing.id, &created_at.to_rfc3339())
                        .await?;

                    return Ok(());
                }

                /*
                 * The database row is malformed or its image
                 * file disappeared.
                 *
                 * Remove the stale row and allow this fresh
                 * clipboard event to repair it.
                 */
                self.repository.delete_by_id(&existing.id).await?;

                if let Some(file_path) = existing.file_path.as_deref() {
                    let _ = image_store.delete_image(file_path).await?;
                }
            } else {
                /*
                 * Transitional compatibility for existing
                 * tests that construct a text-only service
                 * without ImageStore.
                 *
                 * Production always configures ImageStore.
                 */
                self.repository.delete_by_id(&existing.id).await?;
            }
        }

        let Some(image_store) = self.image_store.as_ref() else {
            /*
             * Preserve the pre-image behavior for isolated
             * tests until all callers have moved to the
             * image-aware constructor path.
             */
            let stored_item = to_stored_image_item(id, None, hash, created_at);

            self.repository.insert(&stored_item).await?;

            self.enforce_limit().await?;

            return Ok(());
        };

        let item_id = id.to_string();

        let file_path = image_store.write_image(&item_id, &image).await?;

        let stored_item = to_stored_image_item(id, Some(file_path.clone()), hash, created_at);

        if let Err(database_error) = self.repository.insert(&stored_item).await {
            match image_store.delete_image(&file_path).await {
                Ok(_) => {
                    return Err(HistoryError::Database(database_error));
                }

                Err(cleanup_error) => {
                    return Err(HistoryError::ImageRollback {
                        database: database_error,
                        cleanup: cleanup_error,
                    });
                }
            }
        }

        self.enforce_limit().await
    }

    async fn enforce_limit(&self) -> Result<(), HistoryError> {
        let count = self.repository.count().await?;

        let max_items = self.config.max_items as i64;

        if count <= max_items {
            return Ok(());
        }

        let excess = count - max_items;

        let oldest_items = self.repository.get_oldest(excess).await?;

        let ids = oldest_items.iter().map(|item| item.id.clone()).collect();

        /*
         * SQLite is the authoritative index.
         *
         * Delete rows first. If a later file deletion fails,
         * startup reconciliation can safely identify that
         * file as orphaned.
         */
        self.repository.delete_by_ids(ids).await?;

        self.cleanup_item_files(&oldest_items).await
    }

    pub async fn get_all(&self) -> Result<Vec<StoredClipboardItem>, HistoryError> {
        Ok(self.repository.get_all().await?)
    }

    pub async fn delete(&self, id: &str) -> Result<bool, HistoryError> {
        let Some(item) = self.repository.get_by_id(id).await? else {
            return Ok(false);
        };

        let deleted = self.repository.delete_by_id(id).await?;

        if !deleted {
            return Ok(false);
        }

        self.cleanup_item_file(&item).await?;

        Ok(true)
    }

    pub async fn clear(&self) -> Result<u64, HistoryError> {
        let items = self.repository.get_all().await?;

        let deleted = self.repository.clear().await?;

        self.cleanup_item_files(&items).await?;

        Ok(deleted)
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<StoredClipboardItem>, HistoryError> {
        Ok(self.repository.get_by_id(id).await?)
    }

    pub async fn promote(&self, id: &str) -> Result<bool, HistoryError> {
        let created_at = chrono::Utc::now().to_rfc3339();

        Ok(self.repository.update_created_at(id, &created_at).await?)
    }

    /// Reconcile image files with SQLite.
    ///
    /// SQLite rows are authoritative.
    ///
    /// Any Pookie-owned image file that is not referenced by
    /// an image row is removed. Stale temporary files are
    /// removed as well.
    pub async fn reconcile_image_store(&self) -> Result<usize, HistoryError> {
        let Some(image_store) = self.image_store.as_ref() else {
            return Ok(0);
        };

        let items = self.repository.get_all().await?;

        let referenced_paths = items
            .into_iter()
            .filter(|item| item.content_type == "image")
            .filter_map(|item| item.file_path)
            .collect::<HashSet<_>>();

        Ok(image_store.cleanup_unreferenced(&referenced_paths).await?)
    }

    async fn cleanup_item_file(&self, item: &StoredClipboardItem) -> Result<(), HistoryError> {
        if item.content_type != "image" {
            return Ok(());
        }

        let Some(file_path) = item.file_path.as_deref() else {
            return Ok(());
        };

        let Some(image_store) = self.image_store.as_ref() else {
            return Ok(());
        };

        image_store.delete_image(file_path).await?;

        Ok(())
    }

    async fn cleanup_item_files(&self, items: &[StoredClipboardItem]) -> Result<(), HistoryError> {
        let mut first_error = None;

        for item in items {
            if let Err(error) = self.cleanup_item_file(item).await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }

        if let Some(error) = first_error {
            return Err(error);
        }

        Ok(())
    }
}
