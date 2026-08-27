use crate::{ClipboardEvent, ClipboardItem, ContentAnalyzer, ContentHasher, ContentNormalizer};

#[derive(Default)]
pub struct ClipboardProcessor;

impl ClipboardProcessor {
    pub fn process(&self, event: ClipboardEvent) -> Option<ClipboardItem> {
        let content = ContentNormalizer::normalize(event.content);

        let metadata = ContentAnalyzer::analyze(&content);

        if metadata.is_empty {
            return None;
        }

        let hash = ContentHasher::hash(&content);

        Some(ClipboardItem::new(content, hash))
    }
}
