use anyhow::Result;

use pookie_clipboard::{ClipboardBackend, wayland::WaylandClipboard, x11::X11Clipboard};

pub enum PlatformClipboard {
    X11(X11Clipboard),

    Wayland(WaylandClipboard),
}

impl PlatformClipboard {
    pub fn new() -> Result<Self> {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

        match session.as_str() {
            "wayland" => Ok(Self::Wayland(WaylandClipboard::new()?)),

            _ => Ok(Self::X11(X11Clipboard::new()?)),
        }
    }
}

impl ClipboardBackend for PlatformClipboard {
    #[allow(dead_code)]
    fn read(&self) -> Result<String, pookie_clipboard::ClipboardError> {
        match self {
            Self::X11(backend) => backend.read(),

            Self::Wayland(backend) => backend.read(),
        }
    }

    #[allow(dead_code)]
    fn write(&self, content: &str) -> Result<(), pookie_clipboard::ClipboardError> {
        match self {
            Self::X11(backend) => backend.write(content),

            Self::Wayland(backend) => backend.write(content),
        }
    }
}

impl PlatformClipboard {
    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11",

            Self::Wayland(_) => "Wayland",
        }
    }
}
