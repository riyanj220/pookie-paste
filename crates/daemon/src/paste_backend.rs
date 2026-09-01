use crate::x11_paste_backend::X11PasteBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteCapability {
    Direct,
    ClipboardOnly,
}

#[derive(Debug)]
pub enum PasteError {
    Unavailable,
    Failed(String),
}

pub trait PasteBackend: Send + Sync {
    fn capability(&self) -> PasteCapability;

    fn paste(&self) -> Result<(), PasteError>;
}

pub struct ClipboardOnlyPasteBackend;

impl PasteBackend for ClipboardOnlyPasteBackend {
    fn capability(&self) -> PasteCapability {
        PasteCapability::ClipboardOnly
    }

    fn paste(&self) -> Result<(), PasteError> {
        Ok(())
    }
}

pub enum PlatformPasteBackend {
    X11(Box<X11PasteBackend>),
    ClipboardOnly(ClipboardOnlyPasteBackend),
}

impl PlatformPasteBackend {
    fn from_session_type(session_type: &str) -> Result<Self, PasteError> {
        match session_type {
            "x11" => Ok(Self::X11(Box::new(X11PasteBackend::new()?))),

            _ => Ok(Self::ClipboardOnly(ClipboardOnlyPasteBackend)),
        }
    }

    pub fn new() -> Result<Self, PasteError> {
        let session_type = std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .to_lowercase();

        Self::from_session_type(&session_type)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11 direct paste",

            Self::ClipboardOnly(_) => "clipboard-only",
        }
    }
}

impl PasteBackend for PlatformPasteBackend {
    fn capability(&self) -> PasteCapability {
        match self {
            Self::X11(backend) => backend.capability(),

            Self::ClipboardOnly(backend) => backend.capability(),
        }
    }

    fn paste(&self) -> Result<(), PasteError> {
        match self {
            Self::X11(backend) => backend.paste(),

            Self::ClipboardOnly(backend) => backend.paste(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_uses_clipboard_only_backend() {
        let backend =
            PlatformPasteBackend::from_session_type("wayland").expect("backend creation failed");

        assert_eq!(backend.capability(), PasteCapability::ClipboardOnly,);
    }

    #[test]
    fn unknown_session_uses_clipboard_only_backend() {
        let backend = PlatformPasteBackend::from_session_type("something-unknown")
            .expect("backend creation failed");

        assert_eq!(backend.capability(), PasteCapability::ClipboardOnly,);
    }

    #[test]
    fn empty_session_uses_clipboard_only_backend() {
        let backend = PlatformPasteBackend::from_session_type("").expect("backend creation failed");

        assert_eq!(backend.capability(), PasteCapability::ClipboardOnly,);
    }
}
