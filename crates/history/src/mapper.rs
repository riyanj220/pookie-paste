use chrono::{DateTime, Utc};
use storage::StoredClipboardItem;
use uuid::Uuid;

pub fn to_stored_text_item(
    id: Uuid,
    text: String,
    hash: String,
    created_at: DateTime<Utc>,
) -> StoredClipboardItem {
    StoredClipboardItem {
        id: id.to_string(),

        content_type: "text".to_string(),

        text_content: Some(text),

        file_path: None,

        content_hash: hash,

        created_at: created_at.to_rfc3339(),

        pinned_at: None,
    }
}

pub fn to_stored_image_item(
    id: Uuid,
    file_path: Option<String>,
    hash: String,
    created_at: DateTime<Utc>,
) -> StoredClipboardItem {
    StoredClipboardItem {
        id: id.to_string(),

        content_type: "image".to_string(),

        text_content: None,

        file_path,

        content_hash: hash,

        created_at: created_at.to_rfc3339(),

        pinned_at: None,
    }
}
