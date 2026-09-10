use anyhow::Result;

use tokio::sync::mpsc::Receiver;

use pookie_clipboard::{
    ClipboardEvent, ClipboardWatcher, WaylandClipboardWatcher, X11ClipboardWatcher,
};

use crate::clipboard_backend::PlatformClipboard;

pub fn start(backend: &PlatformClipboard) -> Result<Receiver<ClipboardEvent>> {
    match backend {
        PlatformClipboard::X11(clipboard) => {
            let mut watcher = X11ClipboardWatcher::new(clipboard.clone());

            Ok(watcher.start())
        }

        PlatformClipboard::Wayland(_) => {
            let mut watcher = WaylandClipboardWatcher::new().map_err(|error| {
                anyhow::anyhow!("failed initializing Wayland clipboard watcher: {}", error)
            })?;

            Ok(watcher.start())
        }
    }
}
