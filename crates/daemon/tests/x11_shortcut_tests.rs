use daemon::shortcut_backend::{
    NamedKey, Shortcut, ShortcutBackend, ShortcutBackendCapability, ShortcutError, ShortcutKey,
    ShortcutModifiers, ShortcutRegistrationOutcome,
};
use daemon::x11_shortcut_backend::{X11ShortcutBackend, shortcut_keysym, x11_modifier_mask};
use x11rb::protocol::xproto::ModMask;

#[test]
fn resolves_ascii_character_keysyms() {
    assert_eq!(shortcut_keysym(ShortcutKey::Character('v')).unwrap(), 0x76);
    assert_eq!(shortcut_keysym(ShortcutKey::Character('V')).unwrap(), 0x76);
    assert_eq!(shortcut_keysym(ShortcutKey::Character('a')).unwrap(), 0x61);
    assert_eq!(shortcut_keysym(ShortcutKey::Character('z')).unwrap(), 0x7a);
    assert_eq!(shortcut_keysym(ShortcutKey::Character('0')).unwrap(), 0x30);
    assert_eq!(shortcut_keysym(ShortcutKey::Character('9')).unwrap(), 0x39);
}

#[test]
fn rejects_non_alphanumeric_character_keysyms() {
    assert!(matches!(
        shortcut_keysym(ShortcutKey::Character('§')),
        Err(ShortcutError::Unavailable)
    ));
    assert!(matches!(
        shortcut_keysym(ShortcutKey::Character('🚀')),
        Err(ShortcutError::Unavailable)
    ));
}

#[test]
fn resolves_named_key_keysyms() {
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::Space)).unwrap(),
        0x0020
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::Tab)).unwrap(),
        0xff09
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::Enter)).unwrap(),
        0xff0d
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::Escape)).unwrap(),
        0xff1b
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::Insert)).unwrap(),
        0xff63
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::Delete)).unwrap(),
        0xffff
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::F(1))).unwrap(),
        0xffbe
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::F(5))).unwrap(),
        0xffc2
    );
    assert_eq!(
        shortcut_keysym(ShortcutKey::Named(NamedKey::F(12))).unwrap(),
        0xffc9
    );
}

#[test]
fn generates_correct_x11_modifier_masks() {
    let empty = x11_modifier_mask(ShortcutModifiers::NONE);
    assert_eq!(empty, ModMask::default());

    let super_only = x11_modifier_mask(ShortcutModifiers {
        super_key: true,
        ..ShortcutModifiers::NONE
    });
    assert_eq!(super_only, ModMask::M4);

    let ctrl_only = x11_modifier_mask(ShortcutModifiers {
        control: true,
        ..ShortcutModifiers::NONE
    });
    assert_eq!(ctrl_only, ModMask::CONTROL);

    let alt_only = x11_modifier_mask(ShortcutModifiers {
        alt: true,
        ..ShortcutModifiers::NONE
    });
    assert_eq!(alt_only, ModMask::M1);

    let shift_only = x11_modifier_mask(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::NONE
    });
    assert_eq!(shift_only, ModMask::SHIFT);

    let all = x11_modifier_mask(ShortcutModifiers {
        super_key: true,
        control: true,
        alt: true,
        shift: true,
    });
    assert_eq!(
        all,
        ModMask::M4 | ModMask::CONTROL | ModMask::M1 | ModMask::SHIFT
    );
}

#[test]
fn live_backend_registration_and_idempotence_if_x11_available() {
    // Attempt connecting to X11 if running in an active X11 session
    let Ok(mut backend) = X11ShortcutBackend::new() else {
        // Headless or non-X11 environment (e.g. Wayland CI)
        return;
    };

    assert_eq!(backend.name(), "X11 global shortcut");
    assert_eq!(backend.capability(), ShortcutBackendCapability::Native);

    let shortcut = Shortcut::super_v();
    let outcome = match backend.register(shortcut) {
        Ok(outcome) => outcome,
        Err(ShortcutError::Conflict(_)) => {
            // Already grabbed by window manager or another app on this system
            return;
        }
        Err(err) => panic!("unexpected registration error: {err:?}"),
    };

    assert!(matches!(
        outcome,
        ShortcutRegistrationOutcome::Active { .. }
    ));
    assert_eq!(backend.registered_shortcut(), Some(shortcut));

    // Test Point 1: Re-registering exact same shortcut is idempotent no-op
    let outcome_again = backend.register(shortcut).expect("idempotent registration");
    assert!(matches!(
        outcome_again,
        ShortcutRegistrationOutcome::Active { .. }
    ));
    assert_eq!(backend.registered_shortcut(), Some(shortcut));

    // Test unregistering
    backend.unregister().expect("clean unregister");
    assert_eq!(backend.registered_shortcut(), None);
}
