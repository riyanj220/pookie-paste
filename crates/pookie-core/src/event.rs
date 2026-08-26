use chrono::{DateTime, Utc};

use pookie_clipboard::ClipboardContent;

#[derive(Debug)]
pub struct ClipboardEvent {
    pub content: ClipboardContent,

    pub created_at: DateTime<Utc>,
}
