use std::collections::HashSet;

use pookie_clipboard::{CanonicalImage, ClipboardContent};
use pookie_core::ClipboardItem;
use storage::{ImageStore, StorageRepository, StoredClipboardItem};

use crate::config::HistoryConfig;
use crate::error::HistoryError;
use crate::mapper::{to_stored_image_item, to_stored_text_item};
use crate::notifier::HistoryRevisionNotifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistorySaveOutcome {
    Inserted { id: String },
    Promoted { id: String },
}

pub struct ClipboardHistoryService {
    repository: StorageRepository,

    config: HistoryConfig,

    image_store: Option<ImageStore>,

    notifier: HistoryRevisionNotifier,
}

impl ClipboardHistoryService {
    pub fn new(repository: StorageRepository, config: HistoryConfig) -> Self {
        Self {
            repository,
            config,
            image_store: None,
            notifier: HistoryRevisionNotifier::default(),
        }
    }

    /// Attach a custom revision notifier (e.g. for testing).
    pub fn with_notifier(mut self, notifier: HistoryRevisionNotifier) -> Self {
        self.notifier = notifier;
        self
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

    pub async fn save(&self, item: ClipboardItem) -> Result<HistorySaveOutcome, HistoryError> {
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
    ) -> Result<HistorySaveOutcome, HistoryError> {
        if let Some(existing) = self.repository.find_by_hash_and_type(&hash, "text").await? {
            let updated = self
                .repository
                .update_created_at(&existing.id, &created_at.to_rfc3339())
                .await?;

            if updated {
                self.notifier.advance();
                return Ok(HistorySaveOutcome::Promoted { id: existing.id });
            }
        }

        let stored_item = to_stored_text_item(id, text, hash, created_at);

        self.repository.insert(&stored_item).await?;

        let enforce_result = self.enforce_limit().await;

        self.notifier.advance();

        enforce_result?;

        Ok(HistorySaveOutcome::Inserted { id: id.to_string() })
    }

    async fn save_image(
        &self,
        id: uuid::Uuid,
        image: CanonicalImage,
        hash: String,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<HistorySaveOutcome, HistoryError> {
        let mut pinned_at = None;

        /*
         * Matching rgba-v1 identity means identical dimensions and
         * RGBA pixels.
         *
         * Reuse the existing owned file and simply move the existing
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
                    let updated = self
                        .repository
                        .update_created_at(&existing.id, &created_at.to_rfc3339())
                        .await?;

                    if updated {
                        self.notifier.advance();
                        return Ok(HistorySaveOutcome::Promoted { id: existing.id });
                    }
                } else {
                    pinned_at = existing.pinned_at;

                    /*
                     * The database row is malformed or its image
                     * file disappeared.
                     *
                     * Remove the stale row and allow this fresh
                     * clipboard event to repair it.
                     */
                    self.repository.delete_by_id(&existing.id).await?;

                    if let Some(file_path) = existing.file_path.as_deref()
                        && let Err(cleanup_err) = image_store.delete_image(file_path).await
                    {
                        tracing::warn!(
                            %cleanup_err,
                            stale_file = file_path,
                            "failed removing stale image file during repair; startup reconciliation will reclaim it"
                        );
                    }
                }
            } else {
                pinned_at = existing.pinned_at;

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
             * Preserve compatibility for isolated tests that
             * construct a history service without ImageStore.
             *
             * Production always configures ImageStore.
             */
            let mut stored_item = to_stored_image_item(id, None, hash, created_at);
            stored_item.pinned_at = pinned_at;

            self.repository.insert(&stored_item).await?;

            let enforce_result = self.enforce_limit().await;

            self.notifier.advance();

            enforce_result?;

            return Ok(HistorySaveOutcome::Inserted { id: id.to_string() });
        };

        let item_id = id.to_string();

        let file_path = image_store.write_image(&item_id, image.png_bytes()).await?;

        let mut stored_item = to_stored_image_item(id, Some(file_path.clone()), hash, created_at);
        stored_item.pinned_at = pinned_at;

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

        let enforce_result = self.enforce_limit().await;

        self.notifier.advance();

        enforce_result?;

        Ok(HistorySaveOutcome::Inserted { id: item_id })
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

        if let Err(cleanup_error) = self.cleanup_item_files(&oldest_items).await {
            tracing::warn!(
                %cleanup_error,
                "failed to clean up image files for pruned history items; startup reconciliation will reclaim orphan files"
            );
        }

        Ok(())
    }

    pub async fn get_all(&self) -> Result<Vec<StoredClipboardItem>, HistoryError> {
        Ok(self.repository.get_all().await?)
    }

    /// Read an authoritative snapshot of all history items paired with the exact
    /// revision they represent.
    ///
    /// Verifies `before_revision == after_revision`. If a concurrent revision-advancing
    /// mutation committed during the database read, it retries automatically until a
    /// provably consistent snapshot is obtained.
    pub async fn get_all_snapshot(&self) -> Result<(Vec<StoredClipboardItem>, u64), HistoryError> {
        loop {
            let before = self.current_revision();
            let items = self.repository.get_all().await?;
            let after = self.current_revision();

            if before == after {
                return Ok((items, after));
            }

            tracing::debug!(
                before,
                after,
                "concurrent history mutation detected during GetHistory snapshot; retrying"
            );
        }
    }

    /// Return the current authoritative revision.
    pub fn current_revision(&self) -> u64 {
        self.notifier.current()
    }

    /// Subscribe to live history revision updates.
    pub fn subscribe_revision(&self) -> tokio::sync::watch::Receiver<u64> {
        self.notifier.subscribe()
    }

    pub async fn delete(&self, id: &str) -> Result<bool, HistoryError> {
        let Some(item) = self.repository.get_by_id(id).await? else {
            return Ok(false);
        };

        let deleted = self.repository.delete_by_id(id).await?;

        if !deleted {
            return Ok(false);
        }

        self.notifier.advance();

        if let Err(cleanup_error) = self.cleanup_item_file(&item).await {
            tracing::warn!(
                id,
                %cleanup_error,
                "failed to clean up image file after SQLite deletion; startup reconciliation will reclaim orphan file"
            );
        }

        Ok(true)
    }

    pub async fn clear(&self) -> Result<u64, HistoryError> {
        let items = self.repository.get_all().await?;

        let deleted = self.repository.clear().await?;

        if deleted > 0 {
            self.notifier.advance();
        }

        if let Err(cleanup_error) = self.cleanup_item_files(&items).await {
            tracing::warn!(
                %cleanup_error,
                "failed to clean up image files after clearing history; startup reconciliation will reclaim orphan files"
            );
        }

        Ok(deleted)
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<StoredClipboardItem>, HistoryError> {
        Ok(self.repository.get_by_id(id).await?)
    }

    /// Load one canonical PNG payload through the history
    /// persistence layer.
    ///
    /// Returns:
    /// ``` text
    ///     Some(bytes) -> ImageStore is configured and the
    ///                    image was read successfully.
    ///
    ///     None        -> This service has no ImageStore.
    /// ```
    /// Missing/invalid image files return a normal
    /// HistoryError rather than crashing the daemon.
    pub async fn read_image_content(
        &self,
        file_path: &str,
    ) -> Result<Option<Vec<u8>>, HistoryError> {
        let Some(image_store) = self.image_store.as_ref() else {
            return Ok(None);
        };

        let image = image_store.read_image(file_path).await?;

        Ok(Some(image))
    }

    pub async fn promote(&self, id: &str) -> Result<bool, HistoryError> {
        let created_at = chrono::Utc::now().to_rfc3339();

        let updated = self.repository.update_created_at(id, &created_at).await?;
        if updated {
            self.notifier.advance();
        }

        Ok(updated)
    }

    pub async fn pin(&self, id: &str) -> Result<bool, HistoryError> {
        let changed = self.repository.pin(id).await?;
        if changed {
            self.notifier.advance();
        }

        Ok(changed)
    }

    pub async fn unpin(&self, id: &str) -> Result<bool, HistoryError> {
        let changed = self.repository.unpin(id).await?;
        if changed {
            self.notifier.advance();
        }

        Ok(changed)
    }

    pub async fn toggle_pin(&self, id: &str) -> Result<Option<bool>, HistoryError> {
        let Some(item) = self.repository.get_by_id(id).await? else {
            return Ok(None);
        };

        if item.pinned_at.is_some() {
            let changed = self.repository.unpin(id).await?;
            if changed {
                self.notifier.advance();
                Ok(Some(false))
            } else {
                Ok(None)
            }
        } else {
            let changed = self.repository.pin(id).await?;
            if changed {
                self.notifier.advance();
                Ok(Some(true))
            } else {
                Ok(None)
            }
        }
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
