#[derive(Debug, sqlx::FromRow)]
pub struct StoredClipboardItem {
    pub id: String,

    pub content_type: String,

    pub text_content: Option<String>,

    pub file_path: Option<String>,

    pub content_hash: String,

    pub created_at: String,
}
