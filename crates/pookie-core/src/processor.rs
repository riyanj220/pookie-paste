use crate::{ClipboardEvent, ClipboardItem, ContentAnalyzer};

#[derive(Default)]
pub struct ClipboardProcessor;

impl ClipboardProcessor {
    pub fn process(&self, event: ClipboardEvent) -> Option<ClipboardItem> {
        let metadata = ContentAnalyzer::analyze(&event.content);

        if metadata.is_empty {
            return None;
        }

        Some(ClipboardItem::new(event.content))
    }
}
