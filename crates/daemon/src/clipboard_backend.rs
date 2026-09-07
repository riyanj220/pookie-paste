use anyhow::Result;

use std::sync::Arc;

use pookie_clipboard::{ClipboardBackend, x11::X11Clipboard};

pub enum PlatformClipboard {
    X11(Arc<X11Clipboard>),

    Wayland,
}

impl PlatformClipboard {
    pub fn new() -> Result<Self> {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

        match session.as_str() {
            "wayland" => Ok(Self::Wayland),

            _ => Ok(Self::X11(Arc::new(X11Clipboard::new()?))),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11",

            Self::Wayland => "Wayland",
        }
    }
}

impl ClipboardBackend for PlatformClipboard {
    fn read(&self) -> Result<String, pookie_clipboard::ClipboardError> {
        match self {
            Self::X11(backend) => backend.read(),

            Self::Wayland => Err(pookie_clipboard::ClipboardError::ReadFailed(
                "Wayland direct clipboard read is not implemented yet".to_string(),
            )),
        }
    }

    fn write(&self, content: &str) -> Result<(), pookie_clipboard::ClipboardError> {
        match self {
            Self::X11(backend) => backend.write(content),

            Self::Wayland => Err(pookie_clipboard::ClipboardError::WriteFailed(
                "Wayland direct clipboard write is not implemented yet".to_string(),
            )),
        }
    }
}
