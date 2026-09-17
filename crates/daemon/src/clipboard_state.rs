use std::sync::Mutex;

use pookie_clipboard::ClipboardContent;
use pookie_core::ContentHasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipboardContentKind {
    Text,

    Image,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipboardFingerprint {
    kind: ClipboardContentKind,

    hash: String,
}

impl ClipboardFingerprint {
    fn from_content(content: &ClipboardContent) -> Self {
        let kind = match content {
            ClipboardContent::Text(_) => ClipboardContentKind::Text,

            ClipboardContent::Image(_) => ClipboardContentKind::Image,
        };

        Self {
            kind,

            hash: ContentHasher::hash(content),
        }
    }
}

#[derive(Default)]
pub struct ClipboardState {
    last_written: Mutex<Option<ClipboardFingerprint>>,
}

impl ClipboardState {
    /// Remember clipboard content written by Pookie.
    ///
    /// Only a compact fingerprint is retained. Large image
    /// payloads are never cloned into ClipboardState.
    pub fn mark_written(&self, content: &ClipboardContent) {
        let fingerprint = ClipboardFingerprint::from_content(content);

        let mut value = self
            .last_written
            .lock()
            .expect("clipboard state mutex poisoned");

        *value = Some(fingerprint);
    }

    /// Return true when this watcher event corresponds to
    /// Pookie's most recent clipboard write.
    ///
    /// A successful match consumes the marker exactly once.
    ///
    /// A non-matching event leaves the marker intact,
    /// preserving the existing self-write suppression
    /// semantics while extending them to images.
    pub fn is_self_write(&self, content: &ClipboardContent) -> bool {
        let fingerprint = ClipboardFingerprint::from_content(content);

        let mut value = self
            .last_written
            .lock()
            .expect("clipboard state mutex poisoned");

        match value.as_ref() {
            Some(last) if last == &fingerprint => {
                *value = None;

                true
            }

            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use pookie_clipboard::ClipboardContent;

    use super::ClipboardState;

    #[test]
    fn matching_text_write_is_suppressed_once() {
        let state = ClipboardState::default();

        let content = ClipboardContent::Text("hello".to_string());

        state.mark_written(&content);

        assert!(state.is_self_write(&content,));

        assert!(!state.is_self_write(&content,));
    }

    #[test]
    fn matching_image_write_is_suppressed_once() {
        let state = ClipboardState::default();

        let content = ClipboardContent::Image(vec![1, 2, 3, 4]);

        state.mark_written(&content);

        assert!(state.is_self_write(&content,));

        assert!(!state.is_self_write(&content,));
    }

    #[test]
    fn different_text_is_not_suppressed() {
        let state = ClipboardState::default();

        state.mark_written(&ClipboardContent::Text("first".to_string()));

        assert!(!state.is_self_write(&ClipboardContent::Text("second".to_string(),),));
    }

    #[test]
    fn different_image_is_not_suppressed() {
        let state = ClipboardState::default();

        state.mark_written(&ClipboardContent::Image(vec![1, 2, 3]));

        assert!(!state.is_self_write(&ClipboardContent::Image(vec![4, 5, 6],),));
    }

    #[test]
    fn non_matching_event_does_not_consume_marker() {
        let state = ClipboardState::default();

        let written = ClipboardContent::Text("pookie".to_string());

        state.mark_written(&written);

        assert!(!state.is_self_write(&ClipboardContent::Text("different".to_string(),),));

        assert!(state.is_self_write(&written,));
    }

    #[test]
    fn text_and_image_are_distinct_even_if_bytes_hash_differently() {
        let state = ClipboardState::default();

        let text = ClipboardContent::Text("abc".to_string());

        let image = ClipboardContent::Image(b"abc".to_vec());

        state.mark_written(&text);

        assert!(!state.is_self_write(&image,));

        assert!(state.is_self_write(&text,));
    }

    #[test]
    fn new_write_replaces_previous_pending_marker() {
        let state = ClipboardState::default();

        let first = ClipboardContent::Text("first".to_string());

        let second = ClipboardContent::Image(vec![1, 2, 3]);

        state.mark_written(&first);

        state.mark_written(&second);

        assert!(!state.is_self_write(&first,));

        assert!(state.is_self_write(&second,));
    }
}
