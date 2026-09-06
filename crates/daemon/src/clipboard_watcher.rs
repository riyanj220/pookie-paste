use anyhow::{Result, anyhow};

use tokio::sync::mpsc::Receiver;

use pookie_clipboard::{ClipboardEvent, ClipboardWatcher, X11ClipboardWatcher};

use crate::clipboard_backend::PlatformClipboard;

pub fn start(backend: &PlatformClipboard) -> Result<Receiver<ClipboardEvent>> {
    match backend {
        PlatformClipboard::X11(clipboard) => {
            let mut watcher = X11ClipboardWatcher::new(clipboard.clone());

            Ok(watcher.start())
        }

        PlatformClipboard::Wayland(_) => {
            Err(anyhow!("Wayland clipboard watcher is not implemented yet"))
        }
    }
}
