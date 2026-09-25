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
