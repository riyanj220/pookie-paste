use std::fs;

use daemon::shortcut_backend::{
    NamedKey, Shortcut, ShortcutBackendCapability, ShortcutKey, ShortcutRegistrationOutcome,
};
use daemon::shortcut_config::{ShortcutConfig, ShortcutConfigError};

#[test]
fn default_config_produces_super_v() {
    let config = ShortcutConfig::default();
    let shortcut = config.primary_shortcut().expect("failed resolving default");

    assert_eq!(shortcut, Shortcut::super_v());
    assert_eq!(shortcut.key, ShortcutKey::Character('v'));
    assert!(shortcut.modifiers.super_key);
    assert!(!shortcut.modifiers.control);
    assert!(!shortcut.modifiers.alt);
    assert!(!shortcut.modifiers.shift);
    assert_eq!(shortcut.to_string(), "Super+V");
}

#[test]
fn parses_standard_primary_table_with_array_modifiers() {
    let toml = r#"
[shortcut.primary]
modifiers = ["SUPER", "SHIFT"]
key = "P"
"#;
    let config = ShortcutConfig::parse_str(toml).expect("parsing failed");
    let shortcut = config.primary_shortcut().expect("conversion failed");

    assert_eq!(shortcut.key, ShortcutKey::Character('p'));
    assert!(shortcut.modifiers.super_key);
    assert!(shortcut.modifiers.shift);
    assert!(!shortcut.modifiers.control);
    assert!(!shortcut.modifiers.alt);
    assert_eq!(shortcut.to_string(), "Super+Shift+P");
}

#[test]
fn parses_single_table_with_string_modifiers() {
    let toml = r#"
[shortcut]
modifiers = "CTRL+ALT"
key = "Space"
"#;
    let config = ShortcutConfig::parse_str(toml).expect("parsing failed");
    let shortcut = config.primary_shortcut().expect("conversion failed");

    assert_eq!(shortcut.key, ShortcutKey::Named(NamedKey::Space));
    assert!(shortcut.modifiers.control);
    assert!(shortcut.modifiers.alt);
    assert!(!shortcut.modifiers.super_key);
    assert!(!shortcut.modifiers.shift);
    assert_eq!(shortcut.to_string(), "Ctrl+Alt+Space");
}

#[test]
fn parses_named_function_keys_and_symbols() {
    let function_keys = [
        ("F1", NamedKey::F(1)),
        ("f5", NamedKey::F(5)),
        ("F12", NamedKey::F(12)),
        ("Tab", NamedKey::Tab),
        ("Enter", NamedKey::Enter),
        ("Escape", NamedKey::Escape),
        ("Insert", NamedKey::Insert),
        ("Delete", NamedKey::Delete),
    ];

    for (key_str, expected_named) in function_keys {
        let toml = format!(
            r#"
[shortcut.primary]
modifiers = ["SUPER"]
key = "{key_str}"
"#
        );
        let config = ShortcutConfig::parse_str(&toml).expect("parsing failed");
        let shortcut = config.primary_shortcut().expect("conversion failed");

        assert_eq!(shortcut.key, ShortcutKey::Named(expected_named));
        assert!(shortcut.modifiers.super_key);
    }
}

#[test]
fn case_insensitive_modifier_parsing() {
    let toml = r#"
[shortcut.primary]
modifiers = ["super", "ctrl", "Alt", "SHIFT"]
key = "v"
"#;
    let config = ShortcutConfig::parse_str(toml).expect("parsing failed");
    let shortcut = config.primary_shortcut().expect("conversion failed");

    assert!(shortcut.modifiers.super_key);
    assert!(shortcut.modifiers.control);
    assert!(shortcut.modifiers.alt);
    assert!(shortcut.modifiers.shift);
    assert_eq!(shortcut.to_string(), "Super+Ctrl+Alt+Shift+V");
}

#[test]
fn alias_modifiers_mod4_and_win() {
    let toml1 = r#"
[shortcut.primary]
modifiers = "mod4"
key = "v"
"#;
    let sc1 = ShortcutConfig::parse_str(toml1)
        .unwrap()
        .primary_shortcut()
        .unwrap();
    assert!(sc1.modifiers.super_key);

    let toml2 = r#"
[shortcut.primary]
modifiers = "win"
key = "v"
"#;
    let sc2 = ShortcutConfig::parse_str(toml2)
        .unwrap()
        .primary_shortcut()
        .unwrap();
    assert!(sc2.modifiers.super_key);
}

#[test]
fn rejects_bare_modifier_less_keys_for_safety() {
    let toml = r#"
[shortcut.primary]
modifiers = []
key = "v"
"#;
    let err = ShortcutConfig::parse_str(toml).expect_err("bare key must be rejected");
    assert!(matches!(err, ShortcutConfigError::ModifierRequired(key) if key == "v"));
}

#[test]
fn rejects_empty_key_string() {
    let toml = r#"
[shortcut.primary]
modifiers = ["SUPER"]
key = "   "
"#;
    let err = ShortcutConfig::parse_str(toml).expect_err("empty key must be rejected");
    assert!(matches!(err, ShortcutConfigError::InvalidKey(_)));
}

#[test]
fn rejects_unknown_modifier() {
    let toml = r#"
[shortcut.primary]
modifiers = ["HYPER"]
key = "V"
"#;
    let err = ShortcutConfig::parse_str(toml).expect_err("unknown modifier must be rejected");
    assert!(matches!(err, ShortcutConfigError::UnknownModifier(m) if m == "hyper"));
}

#[test]
fn loads_from_file_path() {
    let temp_dir = std::env::temp_dir().join(format!("pookie-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let config_file = temp_dir.join("config.toml");

    fs::write(
        &config_file,
        r#"
[shortcut.primary]
modifiers = ["SUPER", "ALT"]
key = "C"
"#,
    )
    .unwrap();

    let loaded = ShortcutConfig::load_from_path(&config_file).expect("file load failed");
    let shortcut = loaded.primary_shortcut().expect("primary shortcut failed");

    assert_eq!(shortcut.to_string(), "Super+Alt+C");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn capability_and_outcome_contracts_are_sound() {
    assert_eq!(
        ShortcutBackendCapability::Native.description(),
        "Native window system key grab (e.g. X11)"
    );
    assert_eq!(
        ShortcutBackendCapability::Portal.description(),
        "Desktop portal global shortcuts (e.g. KDE Plasma)"
    );
    assert_eq!(
        ShortcutBackendCapability::CompositorManaged.description(),
        "Compositor-managed keybinding (e.g. Sway, Hyprland)"
    );
    assert_eq!(
        ShortcutBackendCapability::Unsupported.description(),
        "Global shortcuts unsupported on this session"
    );

    let outcome_active = ShortcutRegistrationOutcome::Active {
        description: "X11 grab active".to_string(),
    };
    assert_eq!(outcome_active.description(), "X11 grab active");

    let outcome_managed = ShortcutRegistrationOutcome::CompositorManaged {
        binding_snippet: "bind = SUPER, V, exec, pookie-paste --toggle".to_string(),
        status: daemon::shortcut_backend::CompositorBindingStatus::Verified,
        conflict: None,
        diagnostic: None,
    };
    assert_eq!(
        outcome_managed.description(),
        "bind = SUPER, V, exec, pookie-paste --toggle"
    );

    let outcome_conflict = ShortcutRegistrationOutcome::Conflict {
        details: "Key already in use".to_string(),
    };
    assert_eq!(outcome_conflict.description(), "Key already in use");
}
