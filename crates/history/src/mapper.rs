use pookie_clipboard::ClipboardContent;
use pookie_core::ClipboardItem;
use storage::StoredClipboardItem;

pub fn to_stored_item(item: ClipboardItem) -> StoredClipboardItem {
    let (content_type, text_content, file_path) = match item.content {
        ClipboardContent::Text(text) => ("text".to_string(), Some(text), None),

        ClipboardContent::Image(_) => ("image".to_string(), None, None),
    };

    StoredClipboardItem {
        id: item.id.to_string(),

        content_type,

        text_content,

        file_path,

        content_hash: item.hash,

        created_at: item.created_at.to_rfc3339(),
    }
}
