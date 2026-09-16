pub use crate::wayland_paste_backend::WaylandPasteBackend;

use crate::portal_eis_paste_backend::PortalEisPasteBackend;
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

    /*
     * Most paste backends own no persistent runtime resources.
     *
     * Portal/EIS overrides this to terminate its worker and
     * emulation session explicitly.
     */
    fn shutdown(&self) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteBackendKind {
    X11,
    Wayland,
    Unknown,
}

fn classify_session_type(session_type: &str) -> PasteBackendKind {
    match session_type.trim().to_ascii_lowercase().as_str() {
        "x11" => PasteBackendKind::X11,
        "wayland" => PasteBackendKind::Wayland,
        _ => PasteBackendKind::Unknown,
    }
}

pub enum PlatformPasteBackend {
    X11(Box<X11PasteBackend>),
    WaylandDirect(PortalEisPasteBackend),
    WaylandFallback(WaylandPasteBackend),
}

impl PlatformPasteBackend {
    pub fn new(allow_wayland_direct: bool) -> Result<Self, PasteError> {
        let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

        Self::from_session_type(&session_type, allow_wayland_direct)
    }

    fn from_session_type(
        session_type: &str,
        allow_wayland_direct: bool,
    ) -> Result<Self, PasteError> {
        match classify_session_type(session_type) {
            PasteBackendKind::X11 => Ok(Self::X11(Box::new(X11PasteBackend::new()?))),

            PasteBackendKind::Wayland => {
                if !allow_wayland_direct {
                    tracing::info!(
                        "Wayland direct paste disabled because focus restoration is unavailable"
                    );

                    return Ok(Self::WaylandFallback(WaylandPasteBackend::new()));
                }

                match PortalEisPasteBackend::new() {
                    Ok(backend) => Ok(Self::WaylandDirect(backend)),

                    Err(error) => {
                        tracing::warn!(
                            error = ?error,
                            "Portal/EIS direct paste unavailable; using clipboard-only fallback"
                        );

                        Ok(Self::WaylandFallback(WaylandPasteBackend::new()))
                    }
                }
            }

            PasteBackendKind::Unknown => Ok(Self::WaylandFallback(WaylandPasteBackend::new())),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11 direct paste",
            Self::WaylandDirect(_) => "Wayland Portal/EIS direct paste",
            Self::WaylandFallback(_) => "Wayland clipboard-only",
        }
    }
}

impl PasteBackend for PlatformPasteBackend {
    fn capability(&self) -> PasteCapability {
        match self {
            Self::X11(backend) => backend.capability(),
            Self::WaylandDirect(backend) => backend.capability(),
            Self::WaylandFallback(backend) => backend.capability(),
        }
    }

    fn paste(&self) -> Result<(), PasteError> {
        match self {
            Self::X11(backend) => backend.paste(),
            Self::WaylandDirect(backend) => backend.paste(),
            Self::WaylandFallback(backend) => backend.paste(),
        }
    }

    fn shutdown(&self) {
        match self {
            Self::X11(backend) => backend.shutdown(),
            Self::WaylandDirect(backend) => backend.shutdown(),
            Self::WaylandFallback(backend) => backend.shutdown(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_wayland_session() {
        assert_eq!(classify_session_type("wayland"), PasteBackendKind::Wayland);
    }

    #[test]
    fn identifies_x11_session() {
        assert_eq!(classify_session_type("x11"), PasteBackendKind::X11);
    }

    #[test]
    fn unknown_session_uses_fallback_classification() {
        assert_eq!(classify_session_type("unknown"), PasteBackendKind::Unknown);
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(classify_session_type("X11"), PasteBackendKind::X11);
        assert_eq!(classify_session_type("WAYLAND"), PasteBackendKind::Wayland);
    }

    #[test]
    fn classification_ignores_whitespace() {
        assert_eq!(classify_session_type("  x11  "), PasteBackendKind::X11);
        assert_eq!(
            classify_session_type("\nwayland\t"),
            PasteBackendKind::Wayland,
        );
    }

    #[test]
    fn wayland_without_focus_restore_stays_clipboard_only() {
        let backend = PlatformPasteBackend::from_session_type("wayland", false)
            .expect("backend creation failed");

        assert_eq!(backend.capability(), PasteCapability::ClipboardOnly);
        assert_eq!(backend.name(), "Wayland clipboard-only");
    }

    #[test]
    fn fallback_backend_remains_clipboard_only() {
        let backend = WaylandPasteBackend::new();

        assert_eq!(backend.capability(), PasteCapability::ClipboardOnly);
    }
}
