use crate::focus_backend::{FocusBackend, FocusError, FocusTarget, UnavailableFocusBackend};

use crate::x11_focus_backend::X11FocusBackend;

pub enum PlatformFocusBackend {
    X11(Box<X11FocusBackend>),
    Unavailable(UnavailableFocusBackend),
}

impl PlatformFocusBackend {
    pub fn new() -> Result<Self, FocusError> {
        let session_type = std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .to_lowercase();

        Self::from_session_type(&session_type)
    }

    fn from_session_type(session_type: &str) -> Result<Self, FocusError> {
        match session_type {
            "x11" => Ok(Self::X11(Box::new(X11FocusBackend::new()?))),

            _ => Ok(Self::Unavailable(UnavailableFocusBackend)),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11 focus",

            Self::Unavailable(_) => "unavailable",
        }
    }
}

impl FocusBackend for PlatformFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        match self {
            Self::X11(backend) => backend.active_target(),

            Self::Unavailable(backend) => backend.active_target(),
        }
    }

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError> {
        match self {
            Self::X11(backend) => backend.restore(target),

            Self::Unavailable(backend) => backend.restore(target),
        }
    }

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError> {
        match self {
            Self::X11(backend) => backend.is_active(target),

            Self::Unavailable(backend) => backend.is_active(target),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus_backend::FocusBackend;

    #[test]
    fn wayland_uses_unavailable_focus_backend() {
        let backend =
            PlatformFocusBackend::from_session_type("wayland").expect("backend creation failed");

        assert!(matches!(
            backend.active_target(),
            Err(FocusError::Unavailable)
        ));
    }

    #[test]
    fn unknown_session_uses_unavailable_focus_backend() {
        let backend =
            PlatformFocusBackend::from_session_type("unknown").expect("backend creation failed");

        assert!(matches!(
            backend.active_target(),
            Err(FocusError::Unavailable)
        ));
    }
}
