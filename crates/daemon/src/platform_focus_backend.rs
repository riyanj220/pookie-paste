use crate::focus_backend::{FocusBackend, FocusError, FocusTarget, UnavailableFocusBackend};

use crate::hyprland_focus_backend::HyprlandFocusBackend;
use crate::kde_focus_backend::KdeFocusBackend;
use crate::sway_focus_backend::SwayFocusBackend;
use crate::x11_focus_backend::X11FocusBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusBackendKind {
    X11,
    Kde,
    Unavailable,
}

fn classify_environment(session_type: &str, current_desktop: &str) -> FocusBackendKind {
    let session = session_type.trim().to_ascii_lowercase();

    if session == "x11" {
        return FocusBackendKind::X11;
    }

    if session != "wayland" {
        return FocusBackendKind::Unavailable;
    }

    let desktop = current_desktop.trim().to_ascii_lowercase();

    let is_kde = desktop.split([':', ';']).any(|part| part.trim() == "kde");

    if is_kde {
        FocusBackendKind::Kde
    } else {
        FocusBackendKind::Unavailable
    }
}

pub enum PlatformFocusBackend {
    X11(Box<X11FocusBackend>),

    Kde(KdeFocusBackend),

    Sway(SwayFocusBackend),

    Hyprland(HyprlandFocusBackend),

    Unavailable(UnavailableFocusBackend),
}

impl PlatformFocusBackend {
    pub fn new() -> Result<Self, FocusError> {
        let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

        let current_desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();

        Self::from_environment(&session_type, &current_desktop)
    }

    fn from_environment(session_type: &str, current_desktop: &str) -> Result<Self, FocusError> {
        match classify_environment(session_type, current_desktop) {
            FocusBackendKind::X11 => Ok(Self::X11(Box::new(X11FocusBackend::new()?))),

            FocusBackendKind::Kde => match KdeFocusBackend::new() {
                Ok(backend) => Ok(Self::Kde(backend)),

                Err(error) => {
                    tracing::warn!(
                        error = ?error,
                        "KDE focus helper unavailable; using focus fallback"
                    );

                    Ok(Self::Unavailable(UnavailableFocusBackend))
                }
            },

            FocusBackendKind::Unavailable => Ok(Self::Unavailable(UnavailableFocusBackend)),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11 focus",

            Self::Kde(_) => "KDE KWin focus",

            Self::Sway(_) => "Sway focus",

            Self::Hyprland(_) => "Hyprland focus",

            Self::Unavailable(_) => "unavailable",
        }
    }

    pub fn can_restore_focus(&self) -> bool {
        matches!(
            self,
            Self::X11(_) | Self::Kde(_) | Self::Sway(_) | Self::Hyprland(_)
        )
    }
}

impl FocusBackend for PlatformFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        match self {
            Self::X11(backend) => backend.active_target(),

            Self::Kde(backend) => backend.active_target(),

            Self::Sway(backend) => backend.active_target(),

            Self::Hyprland(backend) => backend.active_target(),

            Self::Unavailable(backend) => backend.active_target(),
        }
    }

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError> {
        match self {
            Self::X11(backend) => backend.restore(target),

            Self::Kde(backend) => backend.restore(target),

            Self::Sway(backend) => backend.restore(target),

            Self::Hyprland(backend) => backend.restore(target),

            Self::Unavailable(backend) => backend.restore(target),
        }
    }

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError> {
        match self {
            Self::X11(backend) => backend.is_active(target),

            Self::Kde(backend) => backend.is_active(target),

            Self::Sway(backend) => backend.is_active(target),

            Self::Hyprland(backend) => backend.is_active(target),

            Self::Unavailable(backend) => backend.is_active(target),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_selects_x11_backend() {
        assert_eq!(classify_environment("x11", "KDE",), FocusBackendKind::X11,);
    }

    #[test]
    fn kde_wayland_selects_kde_backend() {
        assert_eq!(
            classify_environment("wayland", "KDE",),
            FocusBackendKind::Kde,
        );
    }

    #[test]
    fn kde_desktop_matching_is_case_insensitive() {
        assert_eq!(
            classify_environment("WAYLAND", "kde",),
            FocusBackendKind::Kde,
        );
    }

    #[test]
    fn composite_kde_desktop_is_detected() {
        assert_eq!(
            classify_environment("wayland", "KDE:Plasma",),
            FocusBackendKind::Kde,
        );
    }

    #[test]
    fn non_kde_wayland_uses_fallback() {
        assert_eq!(
            classify_environment("wayland", "GNOME",),
            FocusBackendKind::Unavailable,
        );
    }

    #[test]
    fn unknown_session_uses_fallback() {
        assert_eq!(
            classify_environment("unknown", "KDE",),
            FocusBackendKind::Unavailable,
        );
    }

    #[test]
    fn unavailable_backend_cannot_restore_focus() {
        let backend = PlatformFocusBackend::from_environment("wayland", "GNOME")
            .expect("backend creation failed");

        assert!(!backend.can_restore_focus());
    }

    #[test]
    fn x11_backend_kind_supports_focus_restoration() {
        assert_eq!(classify_environment("x11", "KDE",), FocusBackendKind::X11,);
    }
}
