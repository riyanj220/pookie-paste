use crate::Database;
use crate::StoredClipboardItem;

use sqlx::SqlitePool;

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
                created_at
            )
            VALUES (?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&item.id)
        .bind(&item.content_type)
        .bind(&item.text_content)
        .bind(&item.file_path)
        .bind(&item.content_hash)
        .bind(&item.created_at)
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
                    created_at
                FROM clipboard_items
                ORDER BY created_at DESC
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

    pub async fn get_oldest(&self, limit: i64) -> Result<Vec<StoredClipboardItem>, sqlx::Error> {
        let items = sqlx::query_as::<_, StoredClipboardItem>(
            "
            SELECT
                id,
                content_type,
                text_content,
                file_path,
                content_hash,
                created_at
            FROM clipboard_items
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
            created_at
        FROM clipboard_items
        WHERE content_hash = ?
        LIMIT 1
        ",
        )
        .bind(hash)
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
            created_at
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
}
