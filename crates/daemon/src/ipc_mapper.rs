use ipc::HistoryItem;
use storage::StoredClipboardItem;

pub fn to_history_item(item: StoredClipboardItem) -> HistoryItem {
    HistoryItem {
        id: item.id,
        content_type: item.content_type,
        text_content: item.text_content,
        file_path: item.file_path,
        created_at: item.created_at,
    }
}
