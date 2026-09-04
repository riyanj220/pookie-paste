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

    fn wait_for_activation(&mut self) -> Result<(), ShortcutError>;
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
}
