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
            let mut previous = None::<String>;

            let mut interval = time::interval(POLL_INTERVAL);

            loop {
                interval.tick().await;

                let current = match clipboard.read() {
                    Ok(value) => value,

                    Err(error) => {
                        tracing::debug!(
                            error = ?error,
                            "failed reading X11 clipboard"
                        );

                        continue;
                    }
                };

                if current.is_empty() {
                    continue;
                }

                let changed = match &previous {
                    Some(old) => old != &current,

                    None => true,
                };

                if !changed {
                    continue;
                }

                previous = Some(current.clone());

                let event = ClipboardEvent::new(ClipboardContent::Text(current));

                if sender.send(event).await.is_err() {
                    tracing::debug!("clipboard watcher receiver dropped");

                    break;
                }
            }
        });

        receiver
    }
}
