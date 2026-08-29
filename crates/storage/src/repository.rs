use crate::Database;
use crate::StoredClipboardItem;

pub struct StorageRepository<'a> {
    database: &'a Database,
}

impl<'a> StorageRepository<'a> {
    pub fn new(database: &'a Database) -> Self {
        Self { database }
    }

    pub async fn insert(&self, item: &StoredClipboardItem) -> Result<(), sqlx::Error> {
        sqlx::query(
            "
            INSERT INTO clipboard_items
            (
                id,
                content,
                content_hash,
                content_type,
                created_at
            )
            VALUES (?, ?, ?, ?, ?)
            ",
        )
        .bind(&item.id)
        .bind(&item.content)
        .bind(&item.content_hash)
        .bind(&item.content_type)
        .bind(&item.created_at)
        .execute(self.database.pool())
        .await?;

        Ok(())
    }

    pub async fn get_all(&self) -> Result<Vec<StoredClipboardItem>, sqlx::Error> {
        let items = sqlx::query_as::<_, StoredClipboardItem>(
            "
                SELECT
                    id,
                    content,
                    content_hash,
                    content_type,
                    created_at
                FROM clipboard_items
                ORDER BY created_at DESC
                ",
        )
        .fetch_all(self.database.pool())
        .await?;

        Ok(items)
    }
}
