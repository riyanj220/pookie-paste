use crate::{
    ClipboardEvent, ClipboardItem, ClipboardPolicy, ContentAnalyzer, ContentHasher,
    ContentNormalizer,
};

/// Processes clipboard events into validated clipboard items.
///
/// Responsibilities:
/// - normalize content
/// - analyze content
/// - generate content hash
/// - detect duplicates
pub struct ClipboardProcessor;

impl ClipboardProcessor {
    pub fn new() -> Self {
        Self
    }

    pub fn process(&self, event: ClipboardEvent) -> Option<ClipboardItem> {
        let content = ContentNormalizer::normalize(event.content);

        let metadata = ContentAnalyzer::analyze(&content);

        if !ClipboardPolicy::accept(&metadata) {
            return None;
        }

        let hash = ContentHasher::hash(&content);

        Some(ClipboardItem::new(content, hash))
    }
}

impl Default for ClipboardProcessor {
    fn default() -> Self {
        Self::new()
    }
}
