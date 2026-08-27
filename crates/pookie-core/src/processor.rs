use crate::{
    ClipboardEvent, ClipboardHistory, ClipboardItem, ContentAnalyzer, ContentHasher,
    ContentNormalizer,
};

/// Processes clipboard events into validated clipboard items.
///
/// Responsibilities:
/// - normalize content
/// - analyze content
/// - generate content hash
/// - detect duplicates
pub struct ClipboardProcessor {
    history: ClipboardHistory,
}

impl ClipboardProcessor {
    pub fn new() -> Self {
        Self {
            history: ClipboardHistory::default(),
        }
    }

    pub fn process(&mut self, event: ClipboardEvent) -> Option<ClipboardItem> {
        let content = ContentNormalizer::normalize(event.content);

        let metadata = ContentAnalyzer::analyze(&content);

        if metadata.is_empty {
            return None;
        }
        let hash = ContentHasher::hash(&content);

        if !self.history.check_and_insert(hash.clone()) {
            return None;
        }

        Some(ClipboardItem::new(content, hash))
    }
}
