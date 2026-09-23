use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub key: ShortcutKey,
    pub modifiers: ShortcutModifiers,
}

impl Shortcut {
    pub const fn new(key: ShortcutKey, modifiers: ShortcutModifiers) -> Self {
        Self { key, modifiers }
    }

    pub const fn super_v() -> Self {
        Self::new(
            ShortcutKey::Character('v'),
            ShortcutModifiers {
                super_key: true,
                ..ShortcutModifiers::NONE
            },
        )
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mod_str = self.modifiers.to_string();
        if mod_str.is_empty() {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}+{}", mod_str, self.key)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Character(char),
    Named(NamedKey),
}

impl fmt::Display for ShortcutKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(c) => write!(f, "{}", c.to_ascii_uppercase()),
            Self::Named(named) => write!(f, "{named}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Space,
    Tab,
    Enter,
    Escape,
    Insert,
    Delete,
    F(u8),
}

impl NamedKey {
    /// Returns the standard X11 keysym for this named key.
    pub fn keysym(&self) -> u32 {
        match self {
            Self::Space => 0x0020,
            Self::Tab => 0xff09,
            Self::Enter => 0xff0d,
            Self::Escape => 0xff1b,
            Self::Insert => 0xff63,
            Self::Delete => 0xffff,
            Self::F(n) if (1..=12).contains(n) => 0xffbe + (*n as u32 - 1),
            Self::F(_) => 0x0000,
        }
    }

    /// Returns the XDG Desktop Portal string identifier for this named key.
    pub fn portal_name(&self) -> &'static str {
        match self {
            Self::Space => "space",
            Self::Tab => "Tab",
            Self::Enter => "Return",
            Self::Escape => "Escape",
            Self::Insert => "Insert",
            Self::Delete => "Delete",
            Self::F(1) => "F1",
            Self::F(2) => "F2",
            Self::F(3) => "F3",
            Self::F(4) => "F4",
            Self::F(5) => "F5",
            Self::F(6) => "F6",
            Self::F(7) => "F7",
            Self::F(8) => "F8",
            Self::F(9) => "F9",
            Self::F(10) => "F10",
            Self::F(11) => "F11",
            Self::F(12) => "F12",
            Self::F(_) => "unknown",
        }
    }
}

impl fmt::Display for NamedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Space => write!(f, "Space"),
            Self::Tab => write!(f, "Tab"),
            Self::Enter => write!(f, "Enter"),
            Self::Escape => write!(f, "Escape"),
            Self::Insert => write!(f, "Insert"),
            Self::Delete => write!(f, "Delete"),
            Self::F(n) => write!(f, "F{n}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShortcutModifiers {
    pub super_key: bool,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
}

impl ShortcutModifiers {
    pub const NONE: Self = Self {
        super_key: false,
        control: false,
        alt: false,
        shift: false,
    };

    pub fn has_any(&self) -> bool {
        self.super_key || self.control || self.alt || self.shift
    }
}

impl fmt::Display for ShortcutModifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.super_key {
            parts.push("Super");
        }
        if self.control {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        write!(f, "{}", parts.join("+"))
    }
}

#[derive(Debug, Clone)]
pub struct ShortcutActivation {
    /*
     * Platform-specific activation identifier.
     *
     * For Wayland this is the activation token
     * returned by the global shortcut portal.
     *
     * X11 does not currently need this value.
     */
    pub activation_token: Option<String>,
}

impl ShortcutActivation {
    pub fn none() -> Self {
        Self {
            activation_token: None,
        }
    }

    pub fn with_activation_token(token: String) -> Self {
        Self {
            activation_token: Some(token),
        }
    }
}

#[derive(Debug)]
pub enum ShortcutError {
    Unavailable,
    Cancelled,
    TimedOut(String),
    Conflict(String),
    Failed(String),
}

/// Identifies the underlying mechanism used by a shortcut backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutBackendCapability {
    /// Programmatic direct grab by the daemon process (e.g., X11 XGrabKey).
    Native,

    /// Handled via a desktop portal D-Bus session (e.g., KDE Plasma xdg-desktop-portal).
    Portal,

    /// Managed externally by window manager / compositor keybind (e.g., Sway, Hyprland).
    CompositorManaged,

    /// Global shortcuts are completely unsupported on the active session.
    Unsupported,
}

impl ShortcutBackendCapability {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Native => "Native window system key grab (e.g. X11)",
            Self::Portal => "Desktop portal global shortcuts (e.g. KDE Plasma)",
            Self::CompositorManaged => "Compositor-managed keybinding (e.g. Sway, Hyprland)",
            Self::Unsupported => "Global shortcuts unsupported on this session",
        }
    }
}

/// Rich status outcome returned when registering or inspecting a global shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutRegistrationOutcome {
    /// Actively registered and grabbed by the backend.
    Active { description: String },

    /// Managed externally by window manager or compositor keybinding.
    CompositorManaged {
        binding_snippet: String,
        verified: bool,
        conflict: Option<String>,
    },

    /// The key could not be bound because of an active conflict.
    Conflict { details: String },
}

impl ShortcutRegistrationOutcome {
    pub fn description(&self) -> &str {
        match self {
            Self::Active { description } => description,
            Self::CompositorManaged {
                binding_snippet, ..
            } => binding_snippet,
            Self::Conflict { details } => details,
        }
    }
}

pub trait ShortcutBackend: Send {
    fn name(&self) -> &'static str;

    fn capability(&self) -> ShortcutBackendCapability;

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError>;

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError>;

    fn unregister(&mut self) -> Result<(), ShortcutError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn super_v_has_expected_definition() {
        let shortcut = Shortcut::super_v();

        assert_eq!(shortcut.key, ShortcutKey::Character('v'));
        assert!(shortcut.modifiers.super_key);
        assert!(!shortcut.modifiers.control);
        assert!(!shortcut.modifiers.alt);
        assert!(!shortcut.modifiers.shift);
        assert_eq!(shortcut.to_string(), "Super+V");
    }

    #[test]
    fn shortcut_formatting_works_for_various_combinations() {
        let sc1 = Shortcut::new(
            ShortcutKey::Character('p'),
            ShortcutModifiers {
                super_key: true,
                shift: true,
                ..ShortcutModifiers::NONE
            },
        );
        assert_eq!(sc1.to_string(), "Super+Shift+P");

        let sc2 = Shortcut::new(
            ShortcutKey::Named(NamedKey::Space),
            ShortcutModifiers {
                control: true,
                alt: true,
                ..ShortcutModifiers::NONE
            },
        );
        assert_eq!(sc2.to_string(), "Ctrl+Alt+Space");

        let sc3 = Shortcut::new(
            ShortcutKey::Named(NamedKey::F(12)),
            ShortcutModifiers {
                super_key: true,
                ..ShortcutModifiers::NONE
            },
        );
        assert_eq!(sc3.to_string(), "Super+F12");
    }

    #[test]
    fn named_key_keysyms_and_portal_names() {
        assert_eq!(NamedKey::Space.keysym(), 0x0020);
        assert_eq!(NamedKey::Space.portal_name(), "space");
        assert_eq!(NamedKey::Escape.keysym(), 0xff1b);
        assert_eq!(NamedKey::Escape.portal_name(), "Escape");
        assert_eq!(NamedKey::F(1).keysym(), 0xffbe);
        assert_eq!(NamedKey::F(1).portal_name(), "F1");
        assert_eq!(NamedKey::F(12).keysym(), 0xffc9);
        assert_eq!(NamedKey::F(12).portal_name(), "F12");
    }

    #[test]
    fn empty_activation_has_no_token() {
        let activation = ShortcutActivation::none();
        assert!(activation.activation_token.is_none());
    }

    #[test]
    fn activation_can_store_wayland_token() {
        let activation = ShortcutActivation::with_activation_token("test-token".to_string());
        assert_eq!(activation.activation_token.as_deref(), Some("test-token"));
    }

    #[test]
    fn capability_descriptions_are_meaningful() {
        assert!(
            ShortcutBackendCapability::Native
                .description()
                .contains("X11")
        );
        assert!(
            ShortcutBackendCapability::Portal
                .description()
                .contains("portal")
        );
        assert!(
            ShortcutBackendCapability::CompositorManaged
                .description()
                .contains("Compositor")
        );
    }

    #[test]
    fn outcome_description_returns_payload() {
        let outcome = ShortcutRegistrationOutcome::Active {
            description: "Registered successfully".to_string(),
        };
        assert_eq!(outcome.description(), "Registered successfully");
    }
}
