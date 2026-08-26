use crate::{ClipboardEvent, ClipboardItem};

#[derive(Default)]
pub struct ClipboardProcessor;

impl ClipboardProcessor {
    pub fn process(&self, event: ClipboardEvent) -> ClipboardItem {
        ClipboardItem::new(event.content)
    }
}
