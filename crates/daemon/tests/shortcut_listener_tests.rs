use std::sync::{Arc, Mutex};
use std::time::Duration;

use daemon::hyprland_shortcut_backend::format_hyprland_lua_binding;
use daemon::shortcut_backend::{
    Shortcut, ShortcutActivation, ShortcutBackend, ShortcutBackendCapability, ShortcutError,
    ShortcutKey, ShortcutModifiers, ShortcutRegistrationOutcome,
};
use daemon::shortcut_config::ShortcutConfig;
use daemon::shortcut_listener::ShortcutListener;
use daemon::sway_shortcut_backend::format_sway_binding;
use daemon::wayland_shortcut_backend::portal_trigger;

struct RecordingShortcutBackend {
    registered: Arc<Mutex<Option<Shortcut>>>,
}

impl ShortcutBackend for RecordingShortcutBackend {
    fn name(&self) -> &'static str {
        "Mock Recording Shortcut Backend"
    }

    fn capability(&self) -> ShortcutBackendCapability {
        ShortcutBackendCapability::Native
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        *self.registered.lock().unwrap() = Some(shortcut);
        Ok(ShortcutRegistrationOutcome::Active {
            description: format!("registered {shortcut}"),
        })
    }

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError> {
        // Return unavailable immediately so the background listener thread exits cleanly
        Err(ShortcutError::Unavailable)
    }
}

#[test]
fn configured_primary_shortcut_reaches_backend_registration() {
    let custom_toml = r#"
[shortcut.primary]
modifiers = ["CTRL", "SHIFT"]
key = "P"
"#;
    let config = ShortcutConfig::parse_str(custom_toml).expect("valid custom shortcut TOML");
    let primary_shortcut = config
        .primary_shortcut()
        .expect("primary shortcut resolves");

    let expected_custom = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    assert_eq!(primary_shortcut, expected_custom);
    assert_ne!(primary_shortcut, Shortcut::super_v());

    let recorded = Arc::new(Mutex::new(None));
    let backend = RecordingShortcutBackend {
        registered: Arc::clone(&recorded),
    };

    let _listener = ShortcutListener::start_with_backend_and_shortcut(backend, primary_shortcut);

    // Give background thread a moment to execute register()
    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let actual_registered = recorded.lock().unwrap().expect("registration occurred");
    assert_eq!(actual_registered, expected_custom);
    assert_ne!(actual_registered, Shortcut::super_v());
}

#[test]
fn default_configuration_resolves_and_registers_super_v() {
    let config = ShortcutConfig::default();
    let default_shortcut = config
        .primary_shortcut()
        .expect("default shortcut resolves");
    assert_eq!(default_shortcut, Shortcut::super_v());

    let recorded = Arc::new(Mutex::new(None));
    let backend = RecordingShortcutBackend {
        registered: Arc::clone(&recorded),
    };

    let _listener = ShortcutListener::start_with_backend_and_shortcut(backend, default_shortcut);

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let actual_registered = recorded.lock().unwrap().expect("registration occurred");
    assert_eq!(actual_registered, Shortcut::super_v());
}

#[test]
fn custom_shortcut_converts_correctly_for_kde_portal_trigger() {
    let custom = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let trigger = portal_trigger(custom).expect("portal trigger conversion succeeds");
    assert_eq!(trigger, "<Ctrl><Shift>p");

    let default_trigger = portal_trigger(Shortcut::super_v()).unwrap();
    assert_eq!(default_trigger, "<Super>v");
}

#[test]
fn custom_shortcut_converts_correctly_for_compositor_managed_backends() {
    let custom = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    // Sway format
    assert_eq!(
        format_sway_binding(custom),
        "bindsym Ctrl+Shift+p exec pookie-paste --toggle"
    );

    // Hyprland Lua format
    assert_eq!(
        format_hyprland_lua_binding(custom),
        "hl.bind(\"CTRL + SHIFT + P\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
    );
}

#[test]
fn decodes_portal_shortcuts_with_populated_trigger() {
    use daemon::wayland_shortcut_backend::decode_shortcuts_result;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Str, Value};

    let mut properties = HashMap::new();
    properties.insert(
        "trigger_description".to_string(),
        OwnedValue::from(Str::from("Meta+V")),
    );
    let shortcuts_vec = vec![("clipboard-history".to_string(), properties)];
    let mut results = HashMap::new();
    results.insert(
        "shortcuts".to_string(),
        OwnedValue::try_from(Value::from(shortcuts_vec)).unwrap(),
    );

    let decoded = decode_shortcuts_result(results, "test").unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].id, "clipboard-history");
    assert_eq!(decoded[0].trigger_description.as_deref(), Some("Meta+V"));
}

#[test]
fn decodes_portal_shortcuts_filters_empty_or_whitespace_trigger() {
    use daemon::wayland_shortcut_backend::decode_shortcuts_result;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Str, Value};

    let mut properties = HashMap::new();
    properties.insert(
        "trigger_description".to_string(),
        OwnedValue::from(Str::from("   ")),
    );
    let shortcuts_vec = vec![("clipboard-history".to_string(), properties)];
    let mut results = HashMap::new();
    results.insert(
        "shortcuts".to_string(),
        OwnedValue::try_from(Value::from(shortcuts_vec)).unwrap(),
    );

    let decoded = decode_shortcuts_result(results, "test").unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].id, "clipboard-history");
    assert_eq!(decoded[0].trigger_description, None);
}

#[test]
fn decodes_portal_shortcuts_handles_missing_trigger() {
    use daemon::wayland_shortcut_backend::decode_shortcuts_result;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Value};

    let properties: HashMap<String, OwnedValue> = HashMap::new();
    let shortcuts_vec = vec![("clipboard-history".to_string(), properties)];
    let mut results = HashMap::new();
    results.insert(
        "shortcuts".to_string(),
        OwnedValue::try_from(Value::from(shortcuts_vec)).unwrap(),
    );

    let decoded = decode_shortcuts_result(results, "test").unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].id, "clipboard-history");
    assert_eq!(decoded[0].trigger_description, None);
}

#[test]
fn evaluates_bound_shortcuts_when_requested_matches_effective() {
    use daemon::wayland_shortcut_backend::{BoundShortcut, evaluate_bound_shortcuts};

    let requested = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let bound = vec![BoundShortcut {
        id: "clipboard-history".to_string(),
        trigger_description: Some("Ctrl+Shift+P".to_string()),
    }];

    let (effective, outcome) = evaluate_bound_shortcuts(&bound, requested).unwrap();
    assert_eq!(effective.as_deref(), Some("Ctrl+Shift+P"));
    match outcome {
        ShortcutRegistrationOutcome::Active { description } => {
            assert!(description.contains("Ctrl+Shift+P"));
        }
        _ => panic!("expected active registration outcome"),
    }
}

#[test]
fn evaluates_bound_shortcuts_when_requested_differs_from_portal_persisted() {
    use daemon::wayland_shortcut_backend::{BoundShortcut, evaluate_bound_shortcuts};

    let requested = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let bound = vec![BoundShortcut {
        id: "clipboard-history".to_string(),
        trigger_description: Some("Meta+V".to_string()),
    }];

    let (effective, outcome) = evaluate_bound_shortcuts(&bound, requested).unwrap();
    assert_eq!(effective.as_deref(), Some("Meta+V"));
    match outcome {
        ShortcutRegistrationOutcome::Active { description } => {
            assert!(description.contains("Meta+V"));
            assert!(description.contains("Ctrl+Shift+P"));
        }
        _ => panic!("expected active registration outcome"),
    }
}

#[test]
fn evaluates_bound_shortcuts_when_trigger_description_is_missing() {
    use daemon::wayland_shortcut_backend::{BoundShortcut, evaluate_bound_shortcuts};

    let requested = Shortcut::super_v();

    let bound = vec![BoundShortcut {
        id: "clipboard-history".to_string(),
        trigger_description: None,
    }];

    let (effective, outcome) = evaluate_bound_shortcuts(&bound, requested).unwrap();
    assert_eq!(effective, None);
    match outcome {
        ShortcutRegistrationOutcome::Active { description } => {
            assert!(description.contains("Super+V"));
        }
        _ => panic!("expected active registration outcome"),
    }
}

#[test]
fn evaluates_bound_shortcuts_when_trigger_description_is_empty() {
    use daemon::wayland_shortcut_backend::{BoundShortcut, evaluate_bound_shortcuts};

    let requested = Shortcut::super_v();

    let bound = vec![BoundShortcut {
        id: "clipboard-history".to_string(),
        trigger_description: Some("   ".to_string()),
    }];

    let (effective, outcome) = evaluate_bound_shortcuts(&bound, requested).unwrap();
    assert_eq!(effective, None);
    match outcome {
        ShortcutRegistrationOutcome::Active { description } => {
            assert!(description.contains("Super+V"));
        }
        _ => panic!("expected active registration outcome"),
    }
}

#[test]
fn evaluates_bound_shortcuts_fails_when_id_is_absent() {
    use daemon::wayland_shortcut_backend::{BoundShortcut, evaluate_bound_shortcuts};

    let requested = Shortcut::super_v();

    let bound = vec![BoundShortcut {
        id: "some-other-action".to_string(),
        trigger_description: Some("Super+V".to_string()),
    }];

    let result = evaluate_bound_shortcuts(&bound, requested);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ShortcutError::Unavailable));
}

#[test]
fn decodes_shortcuts_changed_payload_correctly() {
    use daemon::wayland_shortcut_backend::decode_shortcuts_vec;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Str};

    let mut properties = HashMap::new();
    properties.insert(
        "trigger_description".to_string(),
        OwnedValue::from(Str::from("Ctrl+Shift+P")),
    );
    let shortcuts_changed_vec = vec![("clipboard-history".to_string(), properties)];

    let decoded = decode_shortcuts_vec(shortcuts_changed_vec);
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].id, "clipboard-history");
    assert_eq!(
        decoded[0].trigger_description.as_deref(),
        Some("Ctrl+Shift+P")
    );
}
