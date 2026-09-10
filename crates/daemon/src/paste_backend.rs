pub use crate::wayland_paste_backend::WaylandPasteBackend;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteBackendKind {
    X11,
    Wayland,
}

fn classify_session_type(session_type: &str) -> PasteBackendKind {
    match session_type.trim().to_ascii_lowercase().as_str() {
        "x11" => PasteBackendKind::X11,

        "wayland" => PasteBackendKind::Wayland,

        _ => PasteBackendKind::Wayland,
    }
}

pub enum PlatformPasteBackend {
    X11(Box<X11PasteBackend>),

    Wayland(WaylandPasteBackend),
}

impl PlatformPasteBackend {
    fn from_session_type(session_type: &str) -> Result<Self, PasteError> {
        match classify_session_type(session_type) {
            PasteBackendKind::X11 => Ok(Self::X11(Box::new(X11PasteBackend::new()?))),

            PasteBackendKind::Wayland => Ok(Self::Wayland(WaylandPasteBackend::new())),
        }
    }

    pub fn new() -> Result<Self, PasteError> {
        let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

        Self::from_session_type(&session_type)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11 direct paste",

            Self::Wayland(_) => "Wayland clipboard-only",
        }
    }
}

impl PasteBackend for PlatformPasteBackend {
    fn capability(&self) -> PasteCapability {
        match self {
            Self::X11(backend) => backend.capability(),

            Self::Wayland(backend) => backend.capability(),
        }
    }

    fn paste(&self) -> Result<(), PasteError> {
        match self {
            Self::X11(backend) => backend.paste(),

            Self::Wayland(backend) => backend.paste(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_wayland_session() {
        assert_eq!(classify_session_type("wayland"), PasteBackendKind::Wayland,);
    }

    #[test]
    fn identifies_x11_session() {
        assert_eq!(classify_session_type("x11"), PasteBackendKind::X11,);
    }

    #[test]
    fn unknown_session_uses_wayland_fallback() {
        assert_eq!(classify_session_type("unknown"), PasteBackendKind::Wayland,);
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(classify_session_type("X11"), PasteBackendKind::X11,);

        assert_eq!(classify_session_type("WAYLAND"), PasteBackendKind::Wayland,);
    }

    #[test]
    fn classification_ignores_whitespace() {
        assert_eq!(classify_session_type("  x11  "), PasteBackendKind::X11,);

        assert_eq!(
            classify_session_type("\nwayland\t"),
            PasteBackendKind::Wayland,
        );
    }

    #[test]
    fn wayland_backend_remains_clipboard_only() {
        let backend = WaylandPasteBackend::new();

        assert_eq!(backend.capability(), PasteCapability::ClipboardOnly,);
    }
}
