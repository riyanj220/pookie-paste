use std::sync::Arc;

use anyhow::Result;

use pookie_clipboard::{
    ClipboardBackend, ClipboardContent, ClipboardError, wayland::WaylandClipboard,
    x11::X11Clipboard,
};

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
    fn read(&self) -> Result<String, ClipboardError> {
        match self {
            Self::X11(backend) => backend.read(),

            Self::Wayland(backend) => backend.read(),
        }
    }

    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        match self {
            Self::X11(backend) => backend.write(content),

            Self::Wayland(backend) => backend.write(content),
        }
    }

    fn read_content(&self) -> Result<ClipboardContent, ClipboardError> {
        match self {
            /*
             * Delegate the content-aware API rather than
             * relying on PlatformClipboard's default.
             *
             * This is important because X11 and Wayland will
             * override their own image behavior independently.
             */
            Self::X11(backend) => backend.read_content(),

            Self::Wayland(backend) => backend.read_content(),
        }
    }

    fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
        match self {
            Self::X11(backend) => backend.write_content(content),

            Self::Wayland(backend) => backend.write_content(content),
        }
    }
}
