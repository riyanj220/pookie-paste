use std::sync::{Arc, Mutex};
use std::time::Duration;

use daemon::hyprland_shortcut_backend::format_hyprland_lua_binding;
use daemon::shortcut_backend::{
    BackendEvent, CompositorBindingStatus, Shortcut, ShortcutActivation, ShortcutBackend,
    ShortcutBackendCapability, ShortcutError, ShortcutKey, ShortcutModifiers,
    ShortcutRegistrationOutcome,
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

struct ControllableShortcutBackend {
    registered: Arc<Mutex<Option<Shortcut>>>,
    activation_rx: Arc<Mutex<std::sync::mpsc::Receiver<Result<ShortcutActivation, ShortcutError>>>>,
}

impl ShortcutBackend for ControllableShortcutBackend {
    fn name(&self) -> &'static str {
        "Mock Controllable Shortcut Backend"
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
        match self.activation_rx.lock().unwrap().recv() {
            Ok(result) => result,
            Err(_) => Err(ShortcutError::Unavailable),
        }
    }
}

#[test]
fn shortcut_listener_status_transitions_to_active() {
    let recorded = Arc::new(Mutex::new(None));
    let (_tx, rx) = std::sync::mpsc::channel();
    let backend = ControllableShortcutBackend {
        registered: Arc::clone(&recorded),
        activation_rx: Arc::new(Mutex::new(rx)),
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    // Wait for registration
    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let status = listener.status();
    assert_eq!(status.configured_shortcut, "Super+V");
    assert_eq!(
        status.backend_name.as_deref(),
        Some("Mock Controllable Shortcut Backend")
    );
    assert_eq!(status.capability, Some(ipc::IpcShortcutCapability::Native));
    match status.state {
        ipc::IpcShortcutState::Active { description } => {
            assert!(description.contains("registered Super+V"));
        }
        other => panic!("expected active state, got {:?}", other),
    }
}

#[test]
fn shortcut_listener_status_transitions_to_unavailable_when_activation_stream_dies() {
    let recorded = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    let backend = ControllableShortcutBackend {
        registered: Arc::clone(&recorded),
        activation_rx: Arc::new(Mutex::new(rx)),
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Trigger activation stream termination
    tx.send(Err(ShortcutError::Unavailable)).unwrap();

    // Give background thread a moment to update status and break
    for _ in 0..20 {
        if matches!(
            listener.status().state,
            ipc::IpcShortcutState::Unavailable { .. }
        ) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    match listener.status().state {
        ipc::IpcShortcutState::Unavailable { reason } => {
            assert!(reason.contains("unavailable"));
        }
        other => panic!("expected unavailable state, got {:?}", other),
    }
}

#[test]
fn shortcut_listener_status_transitions_to_failed_when_backend_errors() {
    let recorded = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    let backend = ControllableShortcutBackend {
        registered: Arc::clone(&recorded),
        activation_rx: Arc::new(Mutex::new(rx)),
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Trigger backend error
    tx.send(Err(ShortcutError::Failed("device lost".to_string())))
        .unwrap();

    // Give background thread a moment to update status and break
    for _ in 0..20 {
        if matches!(
            listener.status().state,
            ipc::IpcShortcutState::Failed { .. }
        ) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    match listener.status().state {
        ipc::IpcShortcutState::Failed { error } => {
            assert!(error.contains("device lost"));
        }
        other => panic!("expected failed state, got {:?}", other),
    }
}

struct ReloadableMockBackend {
    registered: Arc<Mutex<Option<Shortcut>>>,
    activation_tx: std::sync::mpsc::Sender<Result<ShortcutActivation, ShortcutError>>,
    activation_rx: Arc<Mutex<std::sync::mpsc::Receiver<Result<ShortcutActivation, ShortcutError>>>>,
    should_fail_rebind: Arc<Mutex<bool>>,
}

impl ShortcutBackend for ReloadableMockBackend {
    fn name(&self) -> &'static str {
        "Reloadable Mock Backend"
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
        match self.activation_rx.lock().unwrap().recv() {
            Ok(result) => result,
            Err(_) => Err(ShortcutError::Unavailable),
        }
    }

    fn wake_trigger(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        let tx = self.activation_tx.clone();
        Some(Arc::new(move || {
            let _ = tx.send(Err(ShortcutError::Interrupted));
        }))
    }

    fn rebind(&mut self, shortcut: Shortcut) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        if *self.should_fail_rebind.lock().unwrap() {
            return Err(ShortcutError::Conflict(format!(
                "key {shortcut} in conflict"
            )));
        }
        *self.registered.lock().unwrap() = Some(shortcut);
        Ok(ShortcutRegistrationOutcome::Active {
            description: format!("reloaded {shortcut}"),
        })
    }
}

#[tokio::test]
async fn listener_reload_rebinds_new_shortcut_successfully() {
    let recorded = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    let backend = ReloadableMockBackend {
        registered: Arc::clone(&recorded),
        activation_tx: tx,
        activation_rx: Arc::new(Mutex::new(rx)),
        should_fail_rebind: Arc::new(Mutex::new(false)),
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    // Wait for initial registration
    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(*recorded.lock().unwrap(), Some(Shortcut::super_v()));

    // Reload with a new shortcut
    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    let status = listener
        .reload(new_shortcut)
        .await
        .expect("reload should succeed");

    assert_eq!(status.configured_shortcut, "Ctrl+Shift+P");
    assert_eq!(*recorded.lock().unwrap(), Some(new_shortcut));
    match status.state {
        ipc::IpcShortcutState::Active { description } => {
            assert!(description.contains("reloaded Ctrl+Shift+P"));
        }
        other => panic!("expected active state, got {:?}", other),
    }
}

#[tokio::test]
async fn listener_reload_preserves_old_shortcut_when_rebind_fails() {
    let recorded = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    let should_fail = Arc::new(Mutex::new(false));
    let backend = ReloadableMockBackend {
        registered: Arc::clone(&recorded),
        activation_tx: tx,
        activation_rx: Arc::new(Mutex::new(rx)),
        should_fail_rebind: Arc::clone(&should_fail),
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    // Wait for initial registration
    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(*recorded.lock().unwrap(), Some(Shortcut::super_v()));

    // Make rebind fail
    *should_fail.lock().unwrap() = true;

    let target_shortcut = Shortcut::new(
        ShortcutKey::Character('x'),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    let err = listener
        .reload(target_shortcut)
        .await
        .expect_err("reload must fail when rebind fails");

    assert!(matches!(err, ShortcutError::Conflict(_)));

    // Verify running shortcut and status are 100% PRESERVED
    assert_eq!(*recorded.lock().unwrap(), Some(Shortcut::super_v()));
    let status = listener.status();
    assert_eq!(status.configured_shortcut, "Super+V");
    match status.state {
        ipc::IpcShortcutState::Active { description } => {
            assert!(description.contains("registered Super+V"));
        }
        other => panic!("expected active state preserved, got {:?}", other),
    }
}

#[tokio::test]
async fn reload_coordinator_resets_to_default_super_v_on_missing_config() {
    use daemon::reload_coordinator::ReloadCoordinator;

    let recorded = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    let backend = ReloadableMockBackend {
        registered: Arc::clone(&recorded),
        activation_tx: tx,
        activation_rx: Arc::new(Mutex::new(rx)),
        should_fail_rebind: Arc::new(Mutex::new(false)),
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let temp_dir =
        std::env::temp_dir().join(format!("pookie_reload_missing_{}", uuid::Uuid::new_v4()));
    let missing_config = temp_dir.join("config.toml");
    let coordinator = ReloadCoordinator::with_custom_paths(
        listener.reload_handle(),
        missing_config.clone(),
        missing_config.clone(),
    );

    // When config file doesn't exist, strict reload bootstraps canonical config and resets to default Super+V
    let status = coordinator.reload().await.expect("default reset reload");
    assert_eq!(status.configured_shortcut, "Super+V");
    assert!(
        missing_config.exists(),
        "canonical default config must be bootstrapped when missing"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

struct LateWakeBackend {
    registered: Arc<Mutex<Option<Shortcut>>>,
    wake_sender: Option<std::sync::mpsc::Sender<Result<ShortcutActivation, ShortcutError>>>,
    activation_rx: Option<std::sync::mpsc::Receiver<Result<ShortcutActivation, ShortcutError>>>,
}

impl ShortcutBackend for LateWakeBackend {
    fn name(&self) -> &'static str {
        "Late Wake Backend"
    }

    fn capability(&self) -> ShortcutBackendCapability {
        ShortcutBackendCapability::Native
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        *self.registered.lock().unwrap() = Some(shortcut);
        // Wake mechanism and activation channel are initialized ONLY during register(),
        // mirroring the lifecycle of WaylandShortcutBackend (KDE Plasma portal).
        let (tx, rx) = std::sync::mpsc::channel();
        self.wake_sender = Some(tx);
        self.activation_rx = Some(rx);
        Ok(ShortcutRegistrationOutcome::Active {
            description: format!("registered {shortcut}"),
        })
    }

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError> {
        let rx = self.activation_rx.as_ref().expect("registered before wait");
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ShortcutError::Unavailable),
        }
    }

    fn wake_trigger(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        // Before register(), wake_sender is None and returns None!
        let tx = self.wake_sender.clone()?;
        Some(Arc::new(move || {
            let _ = tx.send(Err(ShortcutError::Interrupted));
        }))
    }

    fn rebind(&mut self, shortcut: Shortcut) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        *self.registered.lock().unwrap() = Some(shortcut);
        Ok(ShortcutRegistrationOutcome::Active {
            description: format!("reloaded {shortcut}"),
        })
    }
}

#[tokio::test]
async fn listener_reload_wakes_backend_whose_wake_trigger_is_initialized_during_register() {
    let recorded = Arc::new(Mutex::new(None));
    let backend = LateWakeBackend {
        registered: Arc::clone(&recorded),
        wake_sender: None,
        activation_rx: None,
    };

    // Verify precondition: before registration, wake_trigger() is None!
    assert!(backend.wake_trigger().is_none());

    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    // Wait for initial registration to complete on the background thread
    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(*recorded.lock().unwrap(), Some(Shortcut::super_v()));

    // Worker is now blocked in wait_for_activation() waiting for events!
    // A reload must wake it via the newly installed wake_trigger.
    // If the wake_trigger was not refreshed after register(), reload() would hang indefinitely.
    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('k'),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );

    let reload_fut = listener.reload(new_shortcut);
    let status = tokio::time::timeout(Duration::from_secs(1), reload_fut)
        .await
        .expect("reload must not hang; worker must be woken via refreshed wake_trigger")
        .expect("reload should succeed");

    assert_eq!(status.configured_shortcut, "Super+K");
    assert_eq!(*recorded.lock().unwrap(), Some(new_shortcut));
}

type RebindHandler =
    Box<dyn FnMut(Shortcut) -> Result<ShortcutRegistrationOutcome, ShortcutError> + Send>;
type PortalHandler = Box<dyn FnMut(Option<&str>) -> Result<Option<String>, ShortcutError> + Send>;

struct FlexibleMockBackend {
    capability: ShortcutBackendCapability,
    registered: Arc<Mutex<Option<Shortcut>>>,
    rebind_handler: Arc<Mutex<RebindHandler>>,
    portal_handler: Arc<Mutex<PortalHandler>>,
    wake_sender: Option<std::sync::mpsc::Sender<Result<ShortcutActivation, ShortcutError>>>,
    activation_rx: Option<std::sync::mpsc::Receiver<Result<ShortcutActivation, ShortcutError>>>,
}

impl ShortcutBackend for FlexibleMockBackend {
    fn name(&self) -> &'static str {
        "Flexible Mock Backend"
    }

    fn capability(&self) -> ShortcutBackendCapability {
        self.capability
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        *self.registered.lock().unwrap() = Some(shortcut);
        let (tx, rx) = std::sync::mpsc::channel();
        self.wake_sender = Some(tx);
        self.activation_rx = Some(rx);
        match self.capability {
            ShortcutBackendCapability::CompositorManaged => {
                Ok(ShortcutRegistrationOutcome::CompositorManaged {
                    binding_snippet: format!("bindsym Mod4+{} exec pookie-paste", shortcut.key),
                    status: CompositorBindingStatus::Unconfigured,
                    conflict: None,
                    diagnostic: None,
                })
            }
            _ => Ok(ShortcutRegistrationOutcome::Active {
                description: format!("registered {shortcut}"),
            }),
        }
    }

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError> {
        let rx = self.activation_rx.as_ref().expect("registered before wait");
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ShortcutError::Unavailable),
        }
    }

    fn wake_trigger(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        let tx = self.wake_sender.clone()?;
        Some(Arc::new(move || {
            let _ = tx.send(Err(ShortcutError::Interrupted));
        }))
    }

    fn rebind(&mut self, shortcut: Shortcut) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        let mut handler = self.rebind_handler.lock().unwrap();
        handler(shortcut)
    }

    fn configure_portal_shortcuts(
        &mut self,
        parent_window: Option<&str>,
    ) -> Result<Option<String>, ShortcutError> {
        let mut handler = self.portal_handler.lock().unwrap();
        handler(parent_window)
    }
}

#[tokio::test]
async fn set_shortcut_native_success() {
    use daemon::reload_coordinator::ReloadCoordinator;

    let recorded = Arc::new(Mutex::new(None));
    let registered_clone = Arc::clone(&recorded);
    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::Native,
        registered: Arc::clone(&recorded),
        rebind_handler: Arc::new(Mutex::new(Box::new(move |shortcut| {
            *registered_clone.lock().unwrap() = Some(shortcut);
            Ok(ShortcutRegistrationOutcome::Active {
                description: format!("rebound {shortcut}"),
            })
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let temp_dir =
        std::env::temp_dir().join(format!("pookie_set_native_ok_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");
    std::fs::write(
        &config_path,
        "# initial comment\n[shortcut.primary]\nmodifiers = [\"SUPER\"]\nkey = \"V\"\n",
    )
    .unwrap();

    let coordinator = ReloadCoordinator::with_custom_paths(
        listener.reload_handle(),
        config_path.clone(),
        config_path.clone(),
    );

    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let status = coordinator
        .set_shortcut(new_shortcut)
        .await
        .expect("set_shortcut succeeds");
    assert_eq!(status.configured_shortcut, "Ctrl+Shift+P");
    assert_eq!(*recorded.lock().unwrap(), Some(new_shortcut));

    // Verify config file was updated and comments were preserved
    let file_content = std::fs::read_to_string(&config_path).unwrap();
    assert!(file_content.contains("# initial comment"));
    assert!(file_content.contains("CTRL"));
    assert!(file_content.contains("P"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn set_shortcut_native_rebind_conflict_leaves_config_and_runtime_old() {
    use daemon::reload_coordinator::{ReloadCoordinator, ReloadError};

    let recorded = Arc::new(Mutex::new(None));
    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::Native,
        registered: Arc::clone(&recorded),
        rebind_handler: Arc::new(Mutex::new(Box::new(|shortcut| {
            Err(ShortcutError::Conflict(format!(
                "shortcut {shortcut} already registered by another application"
            )))
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let temp_dir = std::env::temp_dir().join(format!(
        "pookie_set_native_conflict_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");
    let initial_toml =
        "# pristine comment\n[shortcut.primary]\nmodifiers = [\"SUPER\"]\nkey = \"V\"\n";
    std::fs::write(&config_path, initial_toml).unwrap();

    let coordinator = ReloadCoordinator::with_custom_paths(
        listener.reload_handle(),
        config_path.clone(),
        config_path.clone(),
    );

    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('x'),
        ShortcutModifiers {
            control: true,
            ..ShortcutModifiers::NONE
        },
    );

    let err = coordinator
        .set_shortcut(new_shortcut)
        .await
        .expect_err("rebind conflict fails");
    assert!(matches!(
        err,
        ReloadError::Shortcut(ShortcutError::Conflict(_))
    ));

    // Runtime shortcut preserved
    assert_eq!(*recorded.lock().unwrap(), Some(Shortcut::super_v()));

    // Config file untouched
    let file_content = std::fs::read_to_string(&config_path).unwrap();
    assert_eq!(file_content, initial_toml);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn set_shortcut_native_commit_failure_triggers_runtime_rollback() {
    use daemon::reload_coordinator::{ReloadCoordinator, ReloadError};

    let recorded = Arc::new(Mutex::new(None));
    let registered_clone = Arc::clone(&recorded);

    let temp_dir = std::env::temp_dir().join(format!(
        "pookie_set_native_rollback_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");
    let initial_toml = "[shortcut.primary]\nmodifiers = [\"SUPER\"]\nkey = \"V\"\n";
    std::fs::write(&config_path, initial_toml).unwrap();

    let sabotage_path = config_path.clone();
    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::Native,
        registered: Arc::clone(&recorded),
        rebind_handler: Arc::new(Mutex::new(Box::new(move |shortcut| {
            *registered_clone.lock().unwrap() = Some(shortcut);
            if shortcut != Shortcut::super_v() {
                // Sabotage target config path by replacing it with a non-empty directory!
                // This causes fs::rename(temp_file, config_path) to fail.
                let _ = std::fs::remove_file(&sabotage_path);
                let _ = std::fs::create_dir(&sabotage_path);
                let _ = std::fs::write(sabotage_path.join("blocker"), "block");
            }
            Ok(ShortcutRegistrationOutcome::Active {
                description: format!("bound {shortcut}"),
            })
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let coordinator = ReloadCoordinator::with_custom_paths(
        listener.reload_handle(),
        config_path.clone(),
        config_path.clone(),
    );

    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let err = coordinator
        .set_shortcut(new_shortcut)
        .await
        .expect_err("commit failure must return error");
    assert!(matches!(err, ReloadError::Config(_)));

    // Rollback succeeded: runtime is restored to previous shortcut (Super+V)!
    assert_eq!(*recorded.lock().unwrap(), Some(Shortcut::super_v()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn set_shortcut_native_simulated_rollback_failure_surfaced_explicitly() {
    use daemon::reload_coordinator::{ReloadCoordinator, ReloadError};

    let recorded = Arc::new(Mutex::new(None));
    let registered_clone = Arc::clone(&recorded);

    let temp_dir = std::env::temp_dir().join(format!(
        "pookie_set_native_dblfail_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");
    let initial_toml = "[shortcut.primary]\nmodifiers = [\"SUPER\"]\nkey = \"V\"\n";
    std::fs::write(&config_path, initial_toml).unwrap();

    let sabotage_path = config_path.clone();
    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::Native,
        registered: Arc::clone(&recorded),
        rebind_handler: Arc::new(Mutex::new(Box::new(move |shortcut| {
            if shortcut != Shortcut::super_v() {
                *registered_clone.lock().unwrap() = Some(shortcut);
                // Sabotage target config path to fail rename
                let _ = std::fs::remove_file(&sabotage_path);
                let _ = std::fs::create_dir(&sabotage_path);
                let _ = std::fs::write(sabotage_path.join("blocker"), "block");
                Ok(ShortcutRegistrationOutcome::Active {
                    description: format!("bound {shortcut}"),
                })
            } else {
                // Rollback ALSO fails!
                Err(ShortcutError::Conflict(
                    "original key Super+V is suddenly claimed".to_string(),
                ))
            }
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let coordinator = ReloadCoordinator::with_custom_paths(
        listener.reload_handle(),
        config_path.clone(),
        config_path.clone(),
    );

    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let err = coordinator
        .set_shortcut(new_shortcut)
        .await
        .expect_err("double failure must return error");
    match err {
        ReloadError::Shortcut(ShortcutError::Failed(msg)) => {
            assert!(msg.contains("failed to commit configuration"));
            assert!(msg.contains("rollback to previous runtime shortcut also failed"));
            assert!(
                msg.contains("current runtime shortcut may differ from persisted configuration")
            );
        }
        other => panic!("expected combined explicit error, got {:?}", other),
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn set_shortcut_portal_rejected() {
    use daemon::reload_coordinator::{ReloadCoordinator, ReloadError};

    let recorded = Arc::new(Mutex::new(None));
    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::Portal,
        registered: Arc::clone(&recorded),
        rebind_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Ok(None)))),
        wake_sender: None,
        activation_rx: None,
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let temp_dir =
        std::env::temp_dir().join(format!("pookie_set_portal_rej_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");
    std::fs::write(
        &config_path,
        "[shortcut.primary]\nmodifiers = [\"SUPER\"]\nkey = \"V\"\n",
    )
    .unwrap();

    let coordinator = ReloadCoordinator::with_custom_paths(
        listener.reload_handle(),
        config_path.clone(),
        config_path.clone(),
    );

    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let err = coordinator
        .set_shortcut(new_shortcut)
        .await
        .expect_err("portal must reject SetShortcut");
    match err {
        ReloadError::Shortcut(ShortcutError::Failed(msg)) => {
            assert_eq!(
                msg,
                "Shortcut is managed by the desktop portal; use ConfigurePortalShortcut"
            );
        }
        other => panic!("expected Portal explicit rejection error, got {:?}", other),
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn set_shortcut_compositor_managed_persists_even_when_unconfigured_or_conflict() {
    use daemon::reload_coordinator::ReloadCoordinator;
    use daemon::shortcut_backend::CompositorBindingStatus;

    let recorded = Arc::new(Mutex::new(None));
    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::CompositorManaged,
        registered: Arc::clone(&recorded),
        rebind_handler: Arc::new(Mutex::new(Box::new(|shortcut| {
            Ok(ShortcutRegistrationOutcome::CompositorManaged {
                binding_snippet: format!("bindsym Mod4+{} exec pookie-paste", shortcut.key),
                status: CompositorBindingStatus::Unconfigured,
                conflict: None,
                diagnostic: Some("Key not found in sway config".into()),
            })
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let temp_dir =
        std::env::temp_dir().join(format!("pookie_set_comp_unconf_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");
    std::fs::write(
        &config_path,
        "[shortcut.primary]\nmodifiers = [\"SUPER\"]\nkey = \"V\"\n",
    )
    .unwrap();

    let coordinator = ReloadCoordinator::with_custom_paths(
        listener.reload_handle(),
        config_path.clone(),
        config_path.clone(),
    );

    let new_shortcut = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );

    let status = coordinator
        .set_shortcut(new_shortcut)
        .await
        .expect("compositor managed persists desired shortcut");
    assert_eq!(status.configured_shortcut, "Super+P");
    match status.state {
        ipc::IpcShortcutState::CompositorManaged {
            binding_status,
            snippet,
            ..
        } => {
            assert_eq!(
                binding_status,
                ipc::IpcCompositorBindingStatus::Unconfigured
            );
            assert!(snippet.contains("bindsym Mod4+P"));
        }
        other => panic!("expected CompositorManaged status, got {:?}", other),
    }

    // Verify config.toml was actually updated on disk
    let file_content = std::fs::read_to_string(&config_path).unwrap();
    assert!(file_content.contains("key = \"P\""));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

type ActivationTx =
    Arc<Mutex<Option<std::sync::mpsc::Sender<Result<ShortcutActivation, ShortcutError>>>>>;
type EventTx = Arc<Mutex<Option<std::sync::mpsc::Sender<BackendEvent>>>>;

struct MockPortalBackend {
    registered: Arc<Mutex<Option<Shortcut>>>,
    portal_calls: Arc<Mutex<usize>>,
    event_tx: EventTx,
    activation_tx: ActivationTx,
    activation_rx: Option<std::sync::mpsc::Receiver<Result<ShortcutActivation, ShortcutError>>>,
    wake_sender: Option<std::sync::mpsc::Sender<Result<ShortcutActivation, ShortcutError>>>,
    effective_trigger: Arc<Mutex<Option<String>>>,
    effective_trigger_str: Option<String>,
}

impl MockPortalBackend {
    #[allow(clippy::type_complexity)]
    fn new(initial_trigger: Option<&str>) -> (Self, EventTx, ActivationTx, Arc<Mutex<usize>>) {
        let event_tx = Arc::new(Mutex::new(None));
        let activation_tx = Arc::new(Mutex::new(None));
        let portal_calls = Arc::new(Mutex::new(0));
        let backend = Self {
            registered: Arc::new(Mutex::new(None)),
            portal_calls: Arc::clone(&portal_calls),
            event_tx: Arc::clone(&event_tx),
            activation_tx: Arc::clone(&activation_tx),
            activation_rx: None,
            wake_sender: None,
            effective_trigger: Arc::new(Mutex::new(initial_trigger.map(|s| s.to_string()))),
            effective_trigger_str: initial_trigger.map(|s| s.to_string()),
        };
        (backend, event_tx, activation_tx, portal_calls)
    }
}

impl ShortcutBackend for MockPortalBackend {
    fn name(&self) -> &'static str {
        "Mock Portal Backend"
    }

    fn capability(&self) -> ShortcutBackendCapability {
        ShortcutBackendCapability::Portal
    }

    fn effective_trigger(&self) -> Option<&str> {
        self.effective_trigger_str.as_deref()
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        *self.registered.lock().unwrap() = Some(shortcut);
        let (tx, rx) = std::sync::mpsc::channel();
        self.wake_sender = Some(tx.clone());
        *self.activation_tx.lock().unwrap() = Some(tx);
        self.activation_rx = Some(rx);
        Ok(ShortcutRegistrationOutcome::Active {
            description: format!("Desktop portal global shortcut active ({shortcut})"),
        })
    }

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError> {
        let rx = self.activation_rx.as_ref().expect("registered before wait");
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ShortcutError::Unavailable),
        }
    }

    fn wake_trigger(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        let tx = self.wake_sender.clone()?;
        Some(Arc::new(move || {
            let _ = tx.send(Err(ShortcutError::Interrupted));
        }))
    }

    fn configure_portal_shortcuts(
        &mut self,
        _parent_window: Option<&str>,
    ) -> Result<Option<String>, ShortcutError> {
        *self.portal_calls.lock().unwrap() += 1;
        // Prompt fire-and-return returning current effective trigger
        Ok(self.effective_trigger.lock().unwrap().clone())
    }

    fn set_event_sender(&mut self, sender: std::sync::mpsc::Sender<BackendEvent>) {
        *self.event_tx.lock().unwrap() = Some(sender);
    }

    fn set_effective_trigger(&mut self, trigger: Option<String>) {
        *self.effective_trigger.lock().unwrap() = trigger;
    }
}

#[tokio::test]
async fn configure_portal_returns_promptly_without_blocking() {
    let (backend, _event_tx, _act_tx, portal_calls) = MockPortalBackend::new(Some("Super+V"));
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let start = std::time::Instant::now();
    let status = listener
        .configure_portal(None)
        .await
        .expect("configure_portal succeeds");
    let elapsed = start.elapsed();

    assert!(*portal_calls.lock().unwrap() >= 1);
    assert!(
        elapsed < Duration::from_millis(200),
        "configure_portal must return promptly without blocking, took {elapsed:?}"
    );
    assert_eq!(status.effective_shortcut.as_deref(), Some("Super+V"));
}

#[tokio::test]
async fn portal_preserves_existing_effective_shortcut_when_no_change_event_arrives() {
    let (backend, _event_tx, _act_tx, _) = MockPortalBackend::new(Some("Super+V"));
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let status = listener
        .configure_portal(None)
        .await
        .expect("configure_portal succeeds");
    assert_eq!(status.effective_shortcut.as_deref(), Some("Super+V"));

    // User closed/cancelled dialog: no ShortcutsChanged event is emitted
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        listener.status().effective_shortcut.as_deref(),
        Some("Super+V")
    );
}

#[tokio::test]
async fn shortcuts_changed_updates_effective_shortcut_and_status() {
    let (backend, event_tx, _act_tx, _) = MockPortalBackend::new(Some("Super+V"));
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Simulate KDE emitting ShortcutsChanged in background
    let sender = event_tx
        .lock()
        .unwrap()
        .clone()
        .expect("event sender wired");
    sender
        .send(BackendEvent::ShortcutsChanged {
            effective_shortcut: "Ctrl+Shift+P".to_string(),
        })
        .unwrap();
    listener.wake();

    // Wait briefly for worker to process event
    for _ in 0..20 {
        if listener.status().effective_shortcut.as_deref() == Some("Ctrl+Shift+P") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let status = listener.status();
    assert_eq!(status.effective_shortcut.as_deref(), Some("Ctrl+Shift+P"));
    assert!(matches!(status.state, ipc::IpcShortcutState::Active { .. }));
}

#[tokio::test]
async fn activation_still_flows_while_and_after_portal_configuration() {
    let (backend, _event_tx, act_tx, _) = MockPortalBackend::new(Some("Super+V"));
    let mut listener =
        ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Trigger portal configuration
    listener
        .configure_portal(None)
        .await
        .expect("configure succeeds");

    // Activation occurs while configuration dialog is open
    let act_sender = act_tx
        .lock()
        .unwrap()
        .clone()
        .expect("activation sender wired");
    act_sender
        .send(Ok(ShortcutActivation {
            activation_token: Some("token_kde_123".to_string()),
        }))
        .unwrap();

    let activation = tokio::time::timeout(Duration::from_millis(500), listener.activated())
        .await
        .expect("activation received within timeout")
        .expect("activation present");
    assert_eq!(
        activation.activation_token.as_deref(),
        Some("token_kde_123")
    );
}

#[tokio::test]
async fn repeated_configure_portal_calls_do_not_create_duplicate_listeners() {
    let (backend, event_tx, _act_tx, portal_calls) = MockPortalBackend::new(Some("Super+V"));
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Call configure multiple times
    listener
        .configure_portal(None)
        .await
        .expect("call 1 succeeds");
    listener
        .configure_portal(None)
        .await
        .expect("call 2 succeeds");
    listener
        .configure_portal(None)
        .await
        .expect("call 3 succeeds");

    assert_eq!(*portal_calls.lock().unwrap(), 3);

    // Emitting one ShortcutsChanged event should update cleanly
    let sender = event_tx
        .lock()
        .unwrap()
        .clone()
        .expect("event sender wired");
    sender
        .send(BackendEvent::ShortcutsChanged {
            effective_shortcut: "Alt+F8".to_string(),
        })
        .unwrap();
    listener.wake();

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        listener.status().effective_shortcut.as_deref(),
        Some("Alt+F8")
    );
}

#[test]
fn expected_ipc_peer_disconnect_is_classified_as_client_disconnect() {
    let err_broken_pipe = std::io::Error::from(std::io::ErrorKind::BrokenPipe);
    let err_reset = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
    let err_aborted = std::io::Error::from(std::io::ErrorKind::ConnectionAborted);
    let err_other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

    let is_expected_disconnect = |err: &std::io::Error| {
        matches!(
            err.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
        )
    };

    assert!(is_expected_disconnect(&err_broken_pipe));
    assert!(is_expected_disconnect(&err_reset));
    assert!(is_expected_disconnect(&err_aborted));
    assert!(!is_expected_disconnect(&err_other));
}

#[tokio::test]
async fn listener_recheck_updates_compositor_managed_status_info() {
    use daemon::shortcut_backend::CompositorBindingStatus;

    let recorded = Arc::new(Mutex::new(None));
    let has_binding = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let has_binding_clone = Arc::clone(&has_binding);

    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::CompositorManaged,
        registered: Arc::clone(&recorded),
        rebind_handler: Arc::new(Mutex::new(Box::new(move |shortcut| {
            let status = if has_binding_clone.load(std::sync::atomic::Ordering::SeqCst) {
                CompositorBindingStatus::Verified
            } else {
                CompositorBindingStatus::Unconfigured
            };
            Ok(ShortcutRegistrationOutcome::CompositorManaged {
                binding_snippet: format!("bindsym Mod4+{} exec pookie-paste", shortcut.key),
                status,
                conflict: None,
                diagnostic: None,
            })
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, Shortcut::super_v());

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Initial status is Unconfigured
    let status1 = listener.status();
    match status1.state {
        ipc::IpcShortcutState::CompositorManaged { binding_status, .. } => {
            assert_eq!(
                binding_status,
                ipc::IpcCompositorBindingStatus::Unconfigured
            );
        }
        other => panic!("expected CompositorManaged, got {:?}", other),
    }

    // Simulate external config update and reload
    has_binding.store(true, std::sync::atomic::Ordering::SeqCst);

    // Call recheck without reloading config
    let status2 = listener.recheck().await.expect("recheck succeeds");
    match status2.state {
        ipc::IpcShortcutState::CompositorManaged { binding_status, .. } => {
            assert_eq!(binding_status, ipc::IpcCompositorBindingStatus::Verified);
        }
        other => panic!("expected CompositorManaged, got {:?}", other),
    }
}

#[tokio::test]
async fn rebind_updates_current_shortcut_for_subsequent_recheck_even_when_unconfigured() {
    use daemon::shortcut_backend::CompositorBindingStatus;

    let recorded_shortcuts = Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = Arc::clone(&recorded_shortcuts);

    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::CompositorManaged,
        registered: Arc::new(Mutex::new(None)),
        rebind_handler: Arc::new(Mutex::new(Box::new(move |shortcut| {
            recorded_clone.lock().unwrap().push(shortcut);
            Ok(ShortcutRegistrationOutcome::CompositorManaged {
                binding_snippet: format!("bindsym Mod4+{} exec pookie-paste", shortcut.key),
                status: CompositorBindingStatus::Unconfigured,
                conflict: None,
                diagnostic: None,
            })
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };

    let initial_shortcut = Shortcut::super_v();
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, initial_shortcut);

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Change desired shortcut to Ctrl+Shift+P
    let new_target = Shortcut::new(
        ShortcutKey::Character('P'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let rebind_status = listener.reload(new_target).await.expect("reload succeeds");
    assert_eq!(rebind_status.configured_shortcut, "Ctrl+Shift+P");

    // Clear recorded list and trigger recheck
    recorded_shortcuts.lock().unwrap().clear();
    let _ = listener.recheck().await.expect("recheck succeeds");

    // Verify recheck queried the NEW desired shortcut (Ctrl+Shift+P), NOT Super+V!
    let queried = recorded_shortcuts.lock().unwrap().clone();
    assert_eq!(queried.len(), 1);
    assert_eq!(queried[0], new_target);
}

#[tokio::test]
async fn compositor_managed_rebind_error_updates_current_shortcut_for_subsequent_recheck() {
    use daemon::shortcut_backend::CompositorBindingStatus;

    let queried_shortcuts = Arc::new(Mutex::new(Vec::new()));
    let queried_clone = Arc::clone(&queried_shortcuts);
    let should_fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let should_fail_clone = Arc::clone(&should_fail);

    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::CompositorManaged,
        registered: Arc::new(Mutex::new(None)),
        rebind_handler: Arc::new(Mutex::new(Box::new(move |shortcut| {
            queried_clone.lock().unwrap().push(shortcut);
            if should_fail_clone.load(std::sync::atomic::Ordering::SeqCst) {
                Err(ShortcutError::Failed("Sway IPC socket error".to_string()))
            } else {
                Ok(ShortcutRegistrationOutcome::CompositorManaged {
                    binding_snippet: format!("bindsym Mod4+{} exec pookie-paste", shortcut.key),
                    status: CompositorBindingStatus::Verified,
                    conflict: None,
                    diagnostic: None,
                })
            }
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };

    let initial_shortcut = Shortcut::super_v();
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, initial_shortcut);
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Change desired shortcut from Super+V (A) to Ctrl+Shift+P (B)
    let new_target = Shortcut::new(
        ShortcutKey::Character('P'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    // Initial rebind returns an actual Err because Sway IPC fails
    let rebind_res = listener.reload(new_target).await;
    assert!(
        rebind_res.is_err(),
        "backend rebind returns an actual error"
    );

    // The listener status should reflect the new desired shortcut B and the error state
    let status_after_err = listener.status();
    assert_eq!(status_after_err.configured_shortcut, "Ctrl+Shift+P");
    match status_after_err.state {
        ipc::IpcShortcutState::Failed { error } => {
            assert!(error.contains("Sway IPC socket error"));
        }
        other => panic!("expected Failed state, got {:?}", other),
    }

    // Now Sway IPC recovers
    should_fail.store(false, std::sync::atomic::Ordering::SeqCst);
    queried_shortcuts.lock().unwrap().clear();

    // Later recheck() is called (e.g. from Check Again or ToggleUi)
    let recheck_status = listener
        .recheck()
        .await
        .expect("recheck succeeds once IPC recovers");
    assert_eq!(recheck_status.configured_shortcut, "Ctrl+Shift+P");
    match recheck_status.state {
        ipc::IpcShortcutState::CompositorManaged { binding_status, .. } => {
            assert_eq!(binding_status, ipc::IpcCompositorBindingStatus::Verified);
        }
        other => panic!("expected CompositorManaged Verified, got {:?}", other),
    }

    // Verify recheck inspected B (Ctrl+Shift+P), NOT A (Super+V)
    let queried = queried_shortcuts.lock().unwrap().clone();
    assert_eq!(queried.len(), 1);
    assert_eq!(queried[0], new_target);
}

#[tokio::test]
async fn native_backend_rebind_error_preserves_previous_shortcut_and_status() {
    let queried_shortcuts = Arc::new(Mutex::new(Vec::new()));
    let queried_clone = Arc::clone(&queried_shortcuts);

    let backend = FlexibleMockBackend {
        capability: ShortcutBackendCapability::Native,
        registered: Arc::new(Mutex::new(None)),
        rebind_handler: Arc::new(Mutex::new(Box::new(move |shortcut| {
            queried_clone.lock().unwrap().push(shortcut);
            Err(ShortcutError::Conflict("X11 grab conflict".to_string()))
        }))),
        portal_handler: Arc::new(Mutex::new(Box::new(|_| Err(ShortcutError::Unavailable)))),
        wake_sender: None,
        activation_rx: None,
    };

    let initial_shortcut = Shortcut::super_v();
    let listener = ShortcutListener::start_with_backend_and_shortcut(backend, initial_shortcut);
    tokio::time::sleep(Duration::from_millis(20)).await;

    let initial_status = listener.status();
    assert_eq!(initial_status.configured_shortcut, "Super+V");

    // Attempt to change shortcut from Super+V (A) to Ctrl+Shift+P (B)
    let new_target = Shortcut::new(
        ShortcutKey::Character('P'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );

    let rebind_res = listener.reload(new_target).await;
    assert!(rebind_res.is_err(), "native rebind fails due to conflict");

    // For Native, listener MUST preserve previous shortcut A and its active status
    let status_after_fail = listener.status();
    assert_eq!(status_after_fail.configured_shortcut, "Super+V");
    match status_after_fail.state {
        ipc::IpcShortcutState::Active { .. } => {}
        other => panic!("expected Active state preserved, got {:?}", other),
    }

    // A subsequent recheck() must preserve A
    let recheck_status = listener.recheck().await.expect("recheck returns status");
    assert_eq!(recheck_status.configured_shortcut, "Super+V");
}
