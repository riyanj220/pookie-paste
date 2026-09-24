use daemon::shortcut_backend::{
    NamedKey, Shortcut, ShortcutBackend, ShortcutBackendCapability, ShortcutError, ShortcutKey,
    ShortcutModifiers, ShortcutRegistrationOutcome,
};
use daemon::sway_shortcut_backend::{
    SwayBindingDiagnosis, SwayShortcutBackend, diagnose_sway_config, format_sway_binding,
    is_pookie_command,
};

#[test]
fn formats_canonical_sway_binding_syntax() {
    let super_v = Shortcut::super_v();
    assert_eq!(
        format_sway_binding(super_v),
        "bindsym Mod4+v exec pookie-paste --toggle"
    );

    let ctrl_shift_p = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    assert_eq!(
        format_sway_binding(ctrl_shift_p),
        "bindsym Ctrl+Shift+p exec pookie-paste --toggle"
    );

    let super_space = Shortcut::new(
        ShortcutKey::Named(NamedKey::Space),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    assert_eq!(
        format_sway_binding(super_space),
        "bindsym Mod4+space exec pookie-paste --toggle"
    );

    let super_f12 = Shortcut::new(
        ShortcutKey::Named(NamedKey::F(12)),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    assert_eq!(
        format_sway_binding(super_f12),
        "bindsym Mod4+F12 exec pookie-paste --toggle"
    );

    let all_mods = Shortcut::new(
        ShortcutKey::Named(NamedKey::Enter),
        ShortcutModifiers {
            super_key: true,
            control: true,
            alt: true,
            shift: true,
        },
    );
    assert_eq!(
        format_sway_binding(all_mods),
        "bindsym Mod4+Ctrl+Mod1+Shift+Return exec pookie-paste --toggle"
    );
}

#[test]
fn precisely_recognizes_pookie_commands() {
    // Valid invocations
    assert!(is_pookie_command("exec pookie-paste --toggle"));
    assert!(is_pookie_command(
        "exec --no-startup-id pookie-paste --toggle"
    ));
    assert!(is_pookie_command("exec /usr/bin/pookie-paste --toggle"));
    assert!(is_pookie_command("exec pookie-paste -t"));
    assert!(is_pookie_command("pookie-paste --toggle"));
    assert!(is_pookie_command("exec pookie-paste-ui"));
    assert!(is_pookie_command("/usr/local/bin/pookie-paste-ui"));

    // Substrings that must NOT match (prevent false positives)
    assert!(!is_pookie_command("exec some-wrapper pookie-paste-test"));
    assert!(!is_pookie_command("exec pookie-paste-helper"));
    assert!(!is_pookie_command("exec rofi -show run"));
    assert!(!is_pookie_command("exec cliphist list"));
    assert!(!is_pookie_command(""));
}

#[test]
fn diagnoses_direct_and_variable_sway_bindings() {
    let super_v = Shortcut::super_v();

    // 1. Direct binding match
    let config1 = r#"
        # Sway config
        font pango:monospace 10
        bindsym Mod4+v exec pookie-paste --toggle
        bindsym Mod4+Return exec alacritty
    "#;
    assert_eq!(
        diagnose_sway_config(config1, super_v),
        SwayBindingDiagnosis::MatchedPookie
    );

    // 2. Variable substitution match
    let config2 = r#"
        set $mod Mod4
        set $term alacritty
        bindsym $mod+v exec pookie-paste --toggle
    "#;
    assert_eq!(
        diagnose_sway_config(config2, super_v),
        SwayBindingDiagnosis::MatchedPookie
    );

    // 3. Modifier ordering independence with variables
    let ctrl_shift_p = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    let config3 = r#"
        set $primary Ctrl
        bindsym Shift+$primary+p exec pookie-paste --toggle
    "#;
    assert_eq!(
        diagnose_sway_config(config3, ctrl_shift_p),
        SwayBindingDiagnosis::MatchedPookie
    );

    // 4. Flags skipped (--to-code, --release)
    let config4 = r#"
        bindsym --to-code Mod4+v exec pookie-paste --toggle
    "#;
    assert_eq!(
        diagnose_sway_config(config4, super_v),
        SwayBindingDiagnosis::MatchedPookie
    );
}

#[test]
fn diagnoses_conflicts_and_absent_bindings() {
    let super_v = Shortcut::super_v();

    // 1. Conflict on direct binding
    let config_conflict1 = r#"
        bindsym Mod4+v exec rofi -show run
    "#;
    assert_eq!(
        diagnose_sway_config(config_conflict1, super_v),
        SwayBindingDiagnosis::Conflict {
            command: "exec rofi -show run".to_string()
        }
    );

    // 2. Conflict on variable binding
    let config_conflict2 = r#"
        set $mod Mod4
        bindsym $mod+v exec cliphist list | rofi -dmenu
    "#;
    assert_eq!(
        diagnose_sway_config(config_conflict2, super_v),
        SwayBindingDiagnosis::Conflict {
            command: "exec cliphist list | rofi -dmenu".to_string()
        }
    );

    // 3. Absent binding
    let config_empty = r#"
        set $mod Mod4
        bindsym $mod+Return exec foot
        bindsym $mod+q kill
    "#;
    assert_eq!(
        diagnose_sway_config(config_empty, super_v),
        SwayBindingDiagnosis::NotFound
    );

    // 4. Commented-out binding is ignored
    let config_commented = r#"
        # bindsym Mod4+v exec pookie-paste --toggle
        bindsym Mod4+c exec foot
    "#;
    assert_eq!(
        diagnose_sway_config(config_commented, super_v),
        SwayBindingDiagnosis::NotFound
    );
}

#[test]
fn backend_lifecycle_and_capability_offline() {
    let mut backend = SwayShortcutBackend::from_offline_config(None);

    assert_eq!(backend.name(), "Sway compositor-managed shortcut");
    assert_eq!(
        backend.capability(),
        ShortcutBackendCapability::CompositorManaged
    );

    let shortcut = Shortcut::super_v();
    let outcome = backend.register(shortcut).expect("registration succeeds");

    match outcome {
        ShortcutRegistrationOutcome::CompositorManaged {
            binding_snippet,
            verified,
            conflict,
        } => {
            assert_eq!(binding_snippet, "bindsym Mod4+v exec pookie-paste --toggle");
            // Offline without IPC, verified MUST be false
            assert!(!verified);
            assert!(conflict.is_none());
        }
        other => panic!("expected CompositorManaged outcome, got: {other:?}"),
    }

    assert_eq!(backend.registered_shortcut(), Some(shortcut));

    // In CompositorManaged mode, wait_for_activation cleanly returns Unavailable
    // (no fake channels, no thread loops)
    assert!(matches!(
        backend.wait_for_activation(),
        Err(ShortcutError::Unavailable)
    ));

    backend.unregister().expect("clean unregister");
    assert_eq!(backend.registered_shortcut(), None);
}

#[test]
fn offline_sway_backend_empty_config_yields_unverified_no_conflict() {
    for empty_cfg in [None, Some(""), Some("# only comments\n\n")] {
        let mut backend = SwayShortcutBackend::from_offline_config(empty_cfg);
        let outcome = backend
            .register(Shortcut::super_v())
            .expect("registration succeeds");

        match outcome {
            ShortcutRegistrationOutcome::CompositorManaged {
                binding_snippet,
                verified,
                conflict,
            } => {
                assert_eq!(binding_snippet, "bindsym Mod4+v exec pookie-paste --toggle");
                assert!(!verified, "offline must never be verified without live IPC");
                assert!(conflict.is_none(), "empty config should have no conflict");
            }
            other => panic!("expected CompositorManaged outcome, got: {other:?}"),
        }
    }
}

#[test]
fn offline_sway_backend_pookie_binding_yields_unverified_no_conflict() {
    let pookie_configs = [
        "bindsym Mod4+v exec pookie-paste --toggle",
        "set $mod Mod4\nbindsym $mod+v exec pookie-paste --toggle",
        "bindsym Mod4+v exec /usr/bin/pookie-paste -t",
    ];

    for config in pookie_configs {
        let mut backend = SwayShortcutBackend::from_offline_config(Some(config));
        let outcome = backend
            .register(Shortcut::super_v())
            .expect("registration succeeds");

        match outcome {
            ShortcutRegistrationOutcome::CompositorManaged {
                binding_snippet,
                verified,
                conflict,
            } => {
                assert_eq!(binding_snippet, "bindsym Mod4+v exec pookie-paste --toggle");
                // Even though the binding matches in the static file, offline inspection must NOT report verified = true!
                assert!(
                    !verified,
                    "file-only fallback without live IPC must report verified = false"
                );
                assert!(
                    conflict.is_none(),
                    "matching pookie binding must not report conflict"
                );
            }
            other => panic!("expected CompositorManaged outcome, got: {other:?}"),
        }
    }
}

#[test]
fn offline_sway_backend_conflicting_super_v_binding_yields_unverified_with_conflict() {
    // Directly models the distro default on Fedora (/etc/sway/config with bindsym $mod+v splitv)
    let conflicting_configs = [
        ("bindsym Mod4+v splitv", "splitv"),
        ("set $mod Mod4\nbindsym $mod+v splitv", "splitv"),
        (
            "bindsym Mod4+v exec clipman pick -t rofi",
            "exec clipman pick -t rofi",
        ),
    ];

    for (config, conflicting_cmd) in conflicting_configs {
        let mut backend = SwayShortcutBackend::from_offline_config(Some(config));
        let outcome = backend
            .register(Shortcut::super_v())
            .expect("registration succeeds");

        match outcome {
            ShortcutRegistrationOutcome::CompositorManaged {
                binding_snippet,
                verified,
                conflict,
            } => {
                assert_eq!(binding_snippet, "bindsym Mod4+v exec pookie-paste --toggle");
                assert!(!verified);
                assert!(
                    conflict.is_some(),
                    "conflicting binding must report conflict"
                );
                let conflict_str = conflict.unwrap();
                assert!(
                    conflict_str.contains(conflicting_cmd),
                    "conflict message '{conflict_str}' must mention '{conflicting_cmd}'"
                );
            }
            other => panic!("expected CompositorManaged outcome, got: {other:?}"),
        }
    }
}
