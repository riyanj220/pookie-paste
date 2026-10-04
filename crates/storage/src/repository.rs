use std::collections::HashSet;

use crate::Database;
use crate::StoredClipboardItem;

use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageMigrationOutcome {
    SkippedStale,
    UpdatedInPlace {
        id: String,
        new_hash: String,
    },
    Consolidated {
        survivor_id: String,
        survivor_file_path: Option<String>,
        redundant_file_paths: Vec<String>,
        new_hash: String,
    },
}

#[derive(Clone)]
pub struct StorageRepository {
    pool: SqlitePool,
}

impl StorageRepository {
    pub fn new(database: &Database) -> Self {
        Self {
            pool: database.pool().clone(),
        }
    }

    pub async fn insert(&self, item: &StoredClipboardItem) -> Result<(), sqlx::Error> {
        sqlx::query(
            "
            INSERT INTO clipboard_items
            (
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&item.id)
        .bind(&item.content_type)
        .bind(&item.text_content)
        .bind(&item.file_path)
        .bind(&item.content_hash)
        .bind(&item.created_at)
        .bind(&item.pinned_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_all(&self) -> Result<Vec<StoredClipboardItem>, sqlx::Error> {
        let items = sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            ORDER BY pinned_at IS NOT NULL DESC, pinned_at DESC, created_at DESC
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    pub async fn count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            "
            SELECT COUNT(*)
            FROM clipboard_items
            ",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Retrieve oldest unpinned items for history limit enforcement.
    ///
    /// Pinned items are explicitly excluded so they are never evicted
    /// when the history capacity limit is reached.
    pub async fn get_oldest(&self, limit: i64) -> Result<Vec<StoredClipboardItem>, sqlx::Error> {
        let items = sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE pinned_at IS NULL
            ORDER BY created_at ASC
            LIMIT ?
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    pub async fn delete_by_ids(&self, ids: Vec<String>) -> Result<(), sqlx::Error> {
        for id in ids {
            sqlx::query(
                "
                DELETE FROM clipboard_items
                WHERE id = ?
                ",
            )
            .bind(id)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn delete_by_id(&self, id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "
            DELETE FROM clipboard_items
            WHERE id = ?
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn clear(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "
            DELETE FROM clipboard_items
            ",
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn pin(&self, id: &str) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query(
            "
            UPDATE clipboard_items
            SET pinned_at = ?
            WHERE id = ? AND pinned_at IS NULL
            ",
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn unpin(&self, id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "
            UPDATE clipboard_items
            SET pinned_at = NULL
            WHERE id = ? AND pinned_at IS NOT NULL
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn find_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<StoredClipboardItem>, sqlx::Error> {
        sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE content_hash = ?
            LIMIT 1
            ",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn find_by_hash_and_type(
        &self,
        hash: &str,
        content_type: &str,
    ) -> Result<Option<StoredClipboardItem>, sqlx::Error> {
        sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE content_hash = ?
              AND content_type = ?
            LIMIT 1
            ",
        )
        .bind(hash)
        .bind(content_type)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<StoredClipboardItem>, sqlx::Error> {
        sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE id = ?
            LIMIT 1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn update_created_at(&self, id: &str, created_at: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "
            UPDATE clipboard_items
            SET created_at = ?
            WHERE id = ?
            ",
        )
        .bind(created_at)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_legacy_image_candidates(
        &self,
    ) -> Result<Vec<StoredClipboardItem>, sqlx::Error> {
        let items = sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE content_type = 'image'
              AND length(content_hash) = 64
              AND content_hash NOT LIKE '%:%'
            ORDER BY pinned_at IS NOT NULL DESC, pinned_at DESC, created_at DESC
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        let candidates = items
            .into_iter()
            .filter(|item| {
                item.content_hash.len() == 64
                    && !item.content_hash.contains(':')
                    && item
                        .content_hash
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
            .collect();

        Ok(candidates)
    }

    pub async fn find_all_by_hash_and_type(
        &self,
        hash: &str,
        content_type: &str,
    ) -> Result<Vec<StoredClipboardItem>, sqlx::Error> {
        sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE content_hash = ?
              AND content_type = ?
            ORDER BY pinned_at IS NOT NULL DESC, pinned_at DESC, created_at DESC
            ",
        )
        .bind(hash)
        .bind(content_type)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn consolidate_legacy_image_row(
        &self,
        candidate_id: &str,
        expected_legacy_hash: &str,
        target_rgba_v1_hash: &str,
        known_healthy_ids: &HashSet<String>,
    ) -> Result<ImageMigrationOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // 1. Fresh candidate re-validation
        let candidate = sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE id = ?
            LIMIT 1
            ",
        )
        .bind(candidate_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(candidate) = candidate else {
            tx.rollback().await?;
            return Ok(ImageMigrationOutcome::SkippedStale);
        };

        if candidate.content_type != "image" || candidate.content_hash != expected_legacy_hash {
            tx.rollback().await?;
            return Ok(ImageMigrationOutcome::SkippedStale);
        }

        // 2. Query colliding rows for target_rgba_v1_hash
        let collisions = sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at,
                pinned_at
            FROM clipboard_items
            WHERE content_hash = ?
              AND content_type = 'image'
              AND id != ?
            ORDER BY pinned_at IS NOT NULL DESC, pinned_at DESC, created_at DESC
            ",
        )
        .bind(target_rgba_v1_hash)
        .bind(candidate_id)
        .fetch_all(&mut *tx)
        .await?;

        if collisions.is_empty() {
            let result = sqlx::query(
                "
                UPDATE clipboard_items
                SET content_hash = ?
                WHERE id = ? AND content_hash = ?
                ",
            )
            .bind(target_rgba_v1_hash)
            .bind(candidate_id)
            .bind(expected_legacy_hash)
            .execute(&mut *tx)
            .await?;

            if result.rows_affected() == 0 {
                tx.rollback().await?;
                return Ok(ImageMigrationOutcome::SkippedStale);
            }

            tx.commit().await?;

            return Ok(ImageMigrationOutcome::UpdatedInPlace {
                id: candidate_id.to_string(),
                new_hash: target_rgba_v1_hash.to_string(),
            });
        }

        // 3. Collisions present: consolidate candidate and colliding rows
        let mut all_rows = Vec::with_capacity(collisions.len() + 1);
        all_rows.push(candidate);
        all_rows.extend(collisions);

        let healthy_rows: Vec<&StoredClipboardItem> = all_rows
            .iter()
            .filter(|row| known_healthy_ids.contains(&row.id))
            .collect();

        if healthy_rows.is_empty() {
            tx.rollback().await?;
            return Ok(ImageMigrationOutcome::SkippedStale);
        }

        // Survivor selection using fresh SQLite metadata among healthy rows
        let mut sorted_healthy = healthy_rows;
        sorted_healthy.sort_by(|a, b| {
            let a_is_target = a.content_hash == target_rgba_v1_hash;
            let b_is_target = b.content_hash == target_rgba_v1_hash;
            if a_is_target != b_is_target {
                return b_is_target.cmp(&a_is_target);
            }

            let a_pinned = a.pinned_at.is_some();
            let b_pinned = b.pinned_at.is_some();
            if a_pinned != b_pinned {
                return b_pinned.cmp(&a_pinned);
            }

            if let (Some(a_pin), Some(b_pin)) = (&a.pinned_at, &b.pinned_at) {
                let pin_cmp = compare_timestamps(b_pin, a_pin);
                if pin_cmp != std::cmp::Ordering::Equal {
                    return pin_cmp;
                }
            }

            let created_cmp = compare_timestamps(&b.created_at, &a.created_at);
            if created_cmp != std::cmp::Ordering::Equal {
                return created_cmp;
            }

            a.id.cmp(&b.id)
        });

        let survivor = sorted_healthy[0].clone();

        // Timestamp merging across ALL rows in all_rows
        let mut merged_created_at = survivor.created_at.clone();
        for r in &all_rows {
            merged_created_at = max_timestamp(&merged_created_at, &r.created_at);
        }

        let mut merged_pinned_at: Option<String> = None;
        for r in &all_rows {
            if let Some(ref pin) = r.pinned_at {
                merged_pinned_at = match merged_pinned_at {
                    Some(ref current) => Some(max_timestamp(current, pin)),
                    None => Some(pin.clone()),
                };
            }
        }

        // Update survivor
        sqlx::query(
            "
            UPDATE clipboard_items
            SET content_hash = ?,
                created_at = ?,
                pinned_at = ?
            WHERE id = ?
            ",
        )
        .bind(target_rgba_v1_hash)
        .bind(&merged_created_at)
        .bind(&merged_pinned_at)
        .bind(&survivor.id)
        .execute(&mut *tx)
        .await?;

        // Delete redundant rows
        let mut redundant_file_paths = Vec::new();
        for row in all_rows {
            if row.id != survivor.id {
                sqlx::query(
                    "
                    DELETE FROM clipboard_items
                    WHERE id = ?
                    ",
                )
                .bind(&row.id)
                .execute(&mut *tx)
                .await?;

                if let Some(path) = row.file_path
                    && survivor.file_path.as_deref() != Some(&path)
                    && !redundant_file_paths.contains(&path)
                {
                    redundant_file_paths.push(path);
                }
            }
        }

        tx.commit().await?;

        Ok(ImageMigrationOutcome::Consolidated {
            survivor_id: survivor.id,
            survivor_file_path: survivor.file_path,
            redundant_file_paths,
            new_hash: target_rgba_v1_hash.to_string(),
        })
    }
}

fn compare_timestamps(a: &str, b: &str) -> std::cmp::Ordering {
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(dt_a), Ok(dt_b)) => dt_a.cmp(&dt_b),
        _ => a.cmp(b),
    }
}

fn max_timestamp(a: &str, b: &str) -> String {
    if compare_timestamps(a, b).is_ge() {
        a.to_string()
    } else {
        b.to_string()
    }
}
