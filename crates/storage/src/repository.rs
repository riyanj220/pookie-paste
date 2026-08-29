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
        .execute(self.database.pool())
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
        .fetch_all(self.database.pool())
        .await?;

        Ok(items)
    }
}
