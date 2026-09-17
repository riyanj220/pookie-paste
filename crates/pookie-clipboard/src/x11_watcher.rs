use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver};
use tokio::time;

use crate::{
    ClipboardBackend, ClipboardContent, ClipboardEvent, ClipboardWatcher, x11::X11Clipboard,
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub struct X11ClipboardWatcher {
    clipboard: Arc<X11Clipboard>,
}

impl X11ClipboardWatcher {
    pub fn new(clipboard: Arc<X11Clipboard>) -> Self {
        Self { clipboard }
    }
}

impl ClipboardWatcher for X11ClipboardWatcher {
    fn start(&mut self) -> Receiver<ClipboardEvent> {
        let (sender, receiver) = mpsc::channel(100);

        let clipboard = Arc::clone(&self.clipboard);

        tokio::spawn(async move {
            let mut previous = None::<ClipboardContent>;

            let mut interval = time::interval(POLL_INTERVAL);

            loop {
                interval.tick().await;

                let current = match clipboard.read_content() {
                    Ok(value) => value,

                    Err(error) => {
                        tracing::debug!(
                            error = ?error,
                            "failed reading X11 clipboard"
                        );

                        continue;
                    }
                };

                if content_is_empty(&current) {
                    continue;
                }

                if previous.as_ref() == Some(&current) {
                    continue;
                }

                /*
                 * Keep one previous content value so the
                 * polling watcher does not repeatedly
                 * emit the same X11 selection.
                 *
                 * This works for both text and canonical
                 * PNG bytes.
                 */
                previous = Some(current.clone());

                let event = ClipboardEvent::new(current);

                if sender.send(event).await.is_err() {
                    tracing::debug!("clipboard watcher receiver dropped");

                    break;
                }
            }
        });

        receiver
    }
}

fn content_is_empty(content: &ClipboardContent) -> bool {
    match content {
        ClipboardContent::Text(text) => text.is_empty(),

        ClipboardContent::Image(image) => image.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::content_is_empty;

    use crate::ClipboardContent;

    #[test]
    fn empty_text_is_empty_content() {
        assert!(content_is_empty(&ClipboardContent::Text(String::new(),),));
    }

    #[test]
    fn non_empty_text_is_not_empty_content() {
        assert!(!content_is_empty(&ClipboardContent::Text(
            "hello".to_string(),
        ),));
    }

    #[test]
    fn empty_image_is_empty_content() {
        assert!(content_is_empty(&ClipboardContent::Image(Vec::new(),),));
    }

    #[test]
    fn non_empty_image_is_not_empty_content() {
        assert!(!content_is_empty(&ClipboardContent::Image(vec![1, 2, 3],),));
    }
}
