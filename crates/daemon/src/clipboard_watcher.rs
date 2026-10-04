use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc::Receiver;

use pookie_clipboard::{
    ClipboardEvent, ClipboardWatcher, WaylandClipboardWatcher, X11ClipboardWatcher,
    x11::X11Clipboard,
};

use crate::clipboard_backend::PlatformClipboard;

pub enum ClipboardWatcherSession {
    X11(X11ClipboardWatcher),
    Wayland(WaylandClipboardWatcher),
}

pub fn start(
    backend: &PlatformClipboard,
) -> Result<(ClipboardWatcherSession, Receiver<ClipboardEvent>)> {
    match backend {
        PlatformClipboard::X11(_) => {
            // Dedicated watcher-side X11Clipboard instance.
            // Activation/writeback retains its own intact X11Clipboard backend without
            // having its X11TargetReader extracted.
            let watcher_clipboard = Arc::new(X11Clipboard::new()?);
            let mut watcher = X11ClipboardWatcher::new(watcher_clipboard);
            let receiver = watcher.start();

            Ok((ClipboardWatcherSession::X11(watcher), receiver))
        }

        PlatformClipboard::Wayland(_) => {
            let mut watcher = WaylandClipboardWatcher::new().map_err(|error| {
                anyhow::anyhow!("failed initializing Wayland clipboard watcher: {}", error)
            })?;
            let receiver = watcher.start();

            Ok((ClipboardWatcherSession::Wayland(watcher), receiver))
        }
    }
}
