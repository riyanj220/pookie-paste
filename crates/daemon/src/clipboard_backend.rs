use std::sync::Arc;

use anyhow::Result;

use pookie_clipboard::{ClipboardBackend, wayland::WaylandClipboard, x11::X11Clipboard};

pub enum PlatformClipboard {
    X11(Arc<X11Clipboard>),

    Wayland(Arc<WaylandClipboard>),
}

impl PlatformClipboard {
    pub fn new() -> Result<Self> {
        let session = std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();

        match session.as_str() {
            "wayland" => Ok(Self::Wayland(Arc::new(WaylandClipboard::new()))),

            _ => Ok(Self::X11(Arc::new(X11Clipboard::new()?))),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11",

            Self::Wayland(_) => "Wayland",
        }
    }
}

impl ClipboardBackend for PlatformClipboard {
    fn read(&self) -> Result<String, pookie_clipboard::ClipboardError> {
        match self {
            /*
             * Existing X11 behavior remains delegated
             * directly to X11Clipboard.
             */
            Self::X11(backend) => backend.read(),

            Self::Wayland(backend) => backend.read(),
        }
    }

    fn write(&self, content: &str) -> Result<(), pookie_clipboard::ClipboardError> {
        match self {
            /*
             * Existing X11 behavior remains delegated
             * directly to X11Clipboard.
             */
            Self::X11(backend) => backend.write(content),

            Self::Wayland(backend) => backend.write(content),
        }
    }
}
