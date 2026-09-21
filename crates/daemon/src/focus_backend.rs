use std::fmt;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    X11(u64),
    Kde(Uuid),
    Sway(i64),
    Hyprland(String),
}

impl FocusTarget {
    /*
     * Kept for compatibility with the existing code/tests.
     *
     * Historically every FocusTarget was an X11 window ID,
     * so FocusTarget::new() continues to mean X11.
     *
     * New platform-specific code should prefer x11() or kde().
     */
    pub fn new(id: u64) -> Self {
        Self::X11(id)
    }

    pub fn x11(id: u64) -> Self {
        Self::X11(id)
    }

    pub fn kde(id: Uuid) -> Self {
        Self::Kde(id)
    }

    pub fn sway(id: i64) -> Self {
        Self::Sway(id)
    }

    pub fn hyprland(address: impl Into<String>) -> Self {
        Self::Hyprland(address.into())
    }

    pub fn x11_id(&self) -> Option<u64> {
        match self {
            Self::X11(id) => Some(*id),
            _ => None,
        }
    }

    pub fn kde_id(&self) -> Option<Uuid> {
        match self {
            Self::Kde(id) => Some(*id),
            _ => None,
        }
    }

    pub fn sway_id(&self) -> Option<i64> {
        match self {
            Self::Sway(id) => Some(*id),
            _ => None,
        }
    }

    pub fn hyprland_address(&self) -> Option<&str> {
        match self {
            Self::Hyprland(address) => Some(address.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for FocusTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X11(id) => {
                write!(formatter, "x11:{id}")
            }

            Self::Kde(id) => {
                write!(formatter, "kde:{id}")
            }

            Self::Sway(id) => {
                write!(formatter, "sway:{id}")
            }

            Self::Hyprland(address) => {
                write!(formatter, "hyprland:{address}")
            }
        }
    }
}

#[derive(Debug)]
pub enum FocusError {
    Unavailable,
    Failed(String),
}

pub trait FocusBackend: Send + Sync {
    fn active_target(&self) -> Result<FocusTarget, FocusError>;

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError>;

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError>;
}

pub struct UnavailableFocusBackend;

impl FocusBackend for UnavailableFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        Err(FocusError::Unavailable)
    }

    fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
        Err(FocusError::Unavailable)
    }

    fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
        Err(FocusError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_new_constructor_creates_x11_target() {
        let target = FocusTarget::new(42);

        assert_eq!(target, FocusTarget::X11(42));
        assert_eq!(target.x11_id(), Some(42));
        assert_eq!(target.kde_id(), None);
    }

    #[test]
    fn creates_explicit_x11_target() {
        let target = FocusTarget::x11(12345);

        assert_eq!(target.x11_id(), Some(12345));
        assert_eq!(target.kde_id(), None);
    }

    #[test]
    fn creates_kde_target() {
        let id = Uuid::new_v4();

        let target = FocusTarget::kde(id);

        assert_eq!(target.kde_id(), Some(id));
        assert_eq!(target.x11_id(), None);
    }

    #[test]
    fn target_display_includes_platform() {
        let x11 = FocusTarget::x11(99);

        assert_eq!(x11.to_string(), "x11:99");

        let kde_id = Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("valid UUID");

        let kde = FocusTarget::kde(kde_id);

        assert_eq!(kde.to_string(), "kde:12345678-1234-5678-1234-567812345678",);

        let sway = FocusTarget::sway(42);
        assert_eq!(sway.to_string(), "sway:42");
        assert_eq!(sway.sway_id(), Some(42));
        assert_eq!(sway.x11_id(), None);
        assert_eq!(sway.kde_id(), None);
        assert_eq!(sway.hyprland_address(), None);

        let hyprland = FocusTarget::hyprland("0x55a72f1b8a90");
        assert_eq!(hyprland.to_string(), "hyprland:0x55a72f1b8a90");
        assert_eq!(hyprland.hyprland_address(), Some("0x55a72f1b8a90"));
        assert_eq!(hyprland.sway_id(), None);
        assert_eq!(hyprland.x11_id(), None);
        assert_eq!(hyprland.kde_id(), None);
    }
}
