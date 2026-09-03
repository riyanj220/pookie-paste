use crate::shortcut_backend::{Shortcut, ShortcutBackend, ShortcutError};

use crate::x11_shortcut_backend::X11ShortcutBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionType {
    X11,
    Wayland,
    Other,
}

pub enum PlatformShortcutBackend {
    X11(Box<X11ShortcutBackend>),
    Unavailable,
}

impl PlatformShortcutBackend {
    pub fn new() -> Result<Self, ShortcutError> {
        let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

        match classify_session_type(&session_type) {
            SessionType::X11 => Ok(Self::X11(Box::new(X11ShortcutBackend::new()?))),

            /*
             * Wayland global shortcuts will later use
             * the XDG Desktop Portal GlobalShortcuts API.
             *
             * For now we explicitly report the capability
             * as unavailable instead of falling back to
             * XWayland/X11 grabs.
             */
            SessionType::Wayland => Ok(Self::Unavailable),

            SessionType::Other => Ok(Self::Unavailable),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11 global shortcut",

            Self::Unavailable => "unavailable",
        }
    }
}

impl ShortcutBackend for PlatformShortcutBackend {
    fn register(&mut self, shortcut: Shortcut) -> Result<(), ShortcutError> {
        match self {
            Self::X11(backend) => backend.register(shortcut),

            Self::Unavailable => Err(ShortcutError::Unavailable),
        }
    }

    fn wait_for_activation(&mut self) -> Result<(), ShortcutError> {
        match self {
            Self::X11(backend) => backend.wait_for_activation(),

            Self::Unavailable => Err(ShortcutError::Unavailable),
        }
    }
}

fn classify_session_type(value: &str) -> SessionType {
    match value.trim().to_ascii_lowercase().as_str() {
        "x11" => SessionType::X11,

        "wayland" => SessionType::Wayland,

        _ => SessionType::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_x11() {
        assert_eq!(classify_session_type("x11",), SessionType::X11,);
    }

    #[test]
    fn identifies_wayland() {
        assert_eq!(classify_session_type("wayland",), SessionType::Wayland,);
    }

    #[test]
    fn unknown_session_is_other() {
        assert_eq!(classify_session_type("tty",), SessionType::Other,);
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(classify_session_type("X11",), SessionType::X11,);

        assert_eq!(classify_session_type("WAYLAND",), SessionType::Wayland,);
    }

    #[test]
    fn classification_ignores_whitespace() {
        assert_eq!(classify_session_type("  x11  ",), SessionType::X11,);
    }
}
