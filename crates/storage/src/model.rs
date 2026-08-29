#[derive(Debug, sqlx::FromRow)]
pub struct StoredClipboardItem {
    pub id: String,

    pub content: String,

    pub content_hash: String,

    pub content_type: String,

    pub created_at: String,
}
