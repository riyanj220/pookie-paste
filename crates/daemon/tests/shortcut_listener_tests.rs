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
    let listener = Arc::new(ShortcutListener::start_with_backend_and_shortcut(
        backend,
        Shortcut::super_v(),
    ));

    for _ in 0..20 {
        if recorded.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let coordinator = ReloadCoordinator::new(Arc::clone(&listener));

    // When config file doesn't exist, strict reload bootstraps canonical config and resets to default Super+V
    let status = coordinator.reload().await.expect("default reset reload");
    assert_eq!(status.configured_shortcut, "Super+V");
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
