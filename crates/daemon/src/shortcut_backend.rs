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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Character(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub trait ShortcutBackend: Send {
    fn register(&mut self, shortcut: Shortcut) -> Result<(), ShortcutError>;

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError>;
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
}
