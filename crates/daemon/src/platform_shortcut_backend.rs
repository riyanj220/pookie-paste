use crate::shortcut_backend::{
    Shortcut, ShortcutActivation, ShortcutBackend, ShortcutBackendCapability, ShortcutError,
    ShortcutRegistrationOutcome,
};

use crate::hyprland_shortcut_backend::HyprlandShortcutBackend;
use crate::sway_shortcut_backend::SwayShortcutBackend;
use crate::wayland_shortcut_backend::WaylandShortcutBackend;
use crate::x11_shortcut_backend::X11ShortcutBackend;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionType {
    X11,
    Wayland,
    Other,
}

pub enum PlatformShortcutBackend {
    X11(Box<X11ShortcutBackend>),
    Sway(Box<SwayShortcutBackend>),
    Hyprland(Box<HyprlandShortcutBackend>),
    Wayland(Box<WaylandShortcutBackend>),
    Unavailable,
}

impl PlatformShortcutBackend {
    pub fn new() -> Result<Self, ShortcutError> {
        let audit = crate::platform::environment::EnvironmentAudit::detect();
        crate::platform::resolvers::resolve_shortcut_backend(&audit)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::X11(b) => b.name(),
            Self::Sway(b) => b.name(),
            Self::Hyprland(b) => b.name(),
            Self::Wayland(b) => b.name(),
            Self::Unavailable => "unavailable",
        }
    }

    pub fn capability(&self) -> ShortcutBackendCapability {
        match self {
            Self::X11(b) => b.capability(),
            Self::Sway(b) => b.capability(),
            Self::Hyprland(b) => b.capability(),
            Self::Wayland(b) => b.capability(),
            Self::Unavailable => ShortcutBackendCapability::Unsupported,
        }
    }
}

impl ShortcutBackend for PlatformShortcutBackend {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn capability(&self) -> ShortcutBackendCapability {
        self.capability()
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        match self {
            Self::X11(backend) => backend.register(shortcut),
            Self::Sway(backend) => backend.register(shortcut),
            Self::Hyprland(backend) => backend.register(shortcut),
            Self::Wayland(backend) => backend.register(shortcut),
            Self::Unavailable => Err(ShortcutError::Unavailable),
        }
    }

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError> {
        match self {
            Self::X11(backend) => backend.wait_for_activation(),
            Self::Sway(backend) => backend.wait_for_activation(),
            Self::Hyprland(backend) => backend.wait_for_activation(),
            Self::Wayland(backend) => backend.wait_for_activation(),
            Self::Unavailable => Err(ShortcutError::Unavailable),
        }
    }

    fn effective_trigger(&self) -> Option<&str> {
        match self {
            Self::Wayland(backend) => backend.effective_trigger(),
            _ => None,
        }
    }

    fn unregister(&mut self) -> Result<(), ShortcutError> {
        match self {
            Self::X11(backend) => backend.unregister(),
            Self::Sway(backend) => backend.unregister(),
            Self::Hyprland(backend) => backend.unregister(),
            Self::Wayland(backend) => backend.unregister(),
            Self::Unavailable => Ok(()),
        }
    }

    fn rebind(&mut self, shortcut: Shortcut) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        match self {
            Self::X11(backend) => backend.rebind(shortcut),
            Self::Sway(backend) => backend.rebind(shortcut),
            Self::Hyprland(backend) => backend.rebind(shortcut),
            Self::Wayland(backend) => backend.rebind(shortcut),
            Self::Unavailable => Err(ShortcutError::Unavailable),
        }
    }

    fn wake_handle(&self) -> Option<std::os::unix::net::UnixStream> {
        match self {
            Self::X11(backend) => backend.wake_handle(),
            Self::Wayland(backend) => backend.wake_handle(),
            _ => None,
        }
    }

    fn wake(&self) -> Result<(), ShortcutError> {
        match self {
            Self::X11(backend) => backend.wake(),
            Self::Wayland(backend) => backend.wake(),
            _ => Ok(()),
        }
    }

    fn wake_trigger(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        match self {
            Self::X11(backend) => backend.wake_trigger(),
            Self::Wayland(backend) => backend.wake_trigger(),
            _ => None,
        }
    }
}

#[allow(dead_code)]
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
        assert_eq!(classify_session_type("x11"), SessionType::X11,);
    }

    #[test]
    fn identifies_wayland() {
        assert_eq!(classify_session_type("wayland"), SessionType::Wayland,);
    }

    #[test]
    fn unknown_session_is_other() {
        assert_eq!(classify_session_type("tty"), SessionType::Other,);
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(classify_session_type("X11"), SessionType::X11,);

        assert_eq!(classify_session_type("WAYLAND"), SessionType::Wayland,);
    }

    #[test]
    fn classification_ignores_whitespace() {
        assert_eq!(classify_session_type("  x11  "), SessionType::X11,);
    }
}
