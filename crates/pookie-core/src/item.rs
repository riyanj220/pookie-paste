use chrono::{DateTime, Utc};

use uuid::Uuid;

use pookie_clipboard::ClipboardContent;

#[derive(Debug)]
pub struct ClipboardItem {
    pub id: Uuid,

    pub content: ClipboardContent,

    pub created_at: DateTime<Utc>,
}

impl ClipboardItem {
    pub fn new(content: ClipboardContent) -> Self {
        Self {
            id: Uuid::new_v4(),

            content,

            created_at: Utc::now(),
        }
    }
}
