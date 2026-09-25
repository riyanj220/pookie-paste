use daemon::hyprland_shortcut_backend::{
    HYPR_MOD_ALT, HYPR_MOD_CTRL, HYPR_MOD_SHIFT, HYPR_MOD_SUPER, HyprlandBindingDiagnosis,
    HyprlandShortcutBackend, diagnose_hyprland_config_text, diagnose_hyprland_ipc_binds,
    format_hyprland_binding, format_hyprland_hyprlang_binding, format_hyprland_lua_binding,
    matches_hyprland_key, shortcut_to_hyprland_modmask,
};
use daemon::shortcut_backend::{
    NamedKey, Shortcut, ShortcutBackend, ShortcutBackendCapability, ShortcutError, ShortcutKey,
    ShortcutModifiers, ShortcutRegistrationOutcome,
};

#[test]
fn formats_canonical_hyprland_binding_syntax() {
    let super_v = Shortcut::super_v();
    // Default canonical format is modern Lua:
    assert_eq!(
        format_hyprland_binding(super_v),
        "hl.bind(\"SUPER + V\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
    );
    assert_eq!(
        format_hyprland_lua_binding(super_v),
        "hl.bind(\"SUPER + V\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
    );
    // Legacy Hyprlang format:
    assert_eq!(
        format_hyprland_hyprlang_binding(super_v),
        "bind = SUPER, V, exec, pookie-paste --toggle"
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
        format_hyprland_binding(ctrl_shift_p),
        "hl.bind(\"CTRL + SHIFT + P\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
    );
    assert_eq!(
        format_hyprland_lua_binding(ctrl_shift_p),
        "hl.bind(\"CTRL + SHIFT + P\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
    );
    assert_eq!(
        format_hyprland_hyprlang_binding(ctrl_shift_p),
        "bind = CTRL SHIFT, P, exec, pookie-paste --toggle"
    );

    let super_space = Shortcut::new(
        ShortcutKey::Named(NamedKey::Space),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    assert_eq!(
        format_hyprland_lua_binding(super_space),
        "hl.bind(\"SUPER + Space\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
    );
    assert_eq!(
        format_hyprland_hyprlang_binding(super_space),
        "bind = SUPER, space, exec, pookie-paste --toggle"
    );

    let super_f12 = Shortcut::new(
        ShortcutKey::Named(NamedKey::F(12)),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    assert_eq!(
        format_hyprland_lua_binding(super_f12),
        "hl.bind(\"SUPER + F12\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
    );
    assert_eq!(
        format_hyprland_hyprlang_binding(super_f12),
        "bind = SUPER, F12, exec, pookie-paste --toggle"
    );
}

#[test]
fn computes_hyprland_modmasks_accurately() {
    assert_eq!(HYPR_MOD_SHIFT, 1);
    assert_eq!(HYPR_MOD_CTRL, 4);
    assert_eq!(HYPR_MOD_ALT, 8);
    assert_eq!(HYPR_MOD_SUPER, 64);

    let super_v = Shortcut::super_v();
    assert_eq!(shortcut_to_hyprland_modmask(super_v), 64);

    let ctrl_shift = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    assert_eq!(shortcut_to_hyprland_modmask(ctrl_shift), 4 | 1); // 5

    let all_mods = Shortcut::new(
        ShortcutKey::Character('a'),
        ShortcutModifiers {
            super_key: true,
            control: true,
            alt: true,
            shift: true,
        },
    );
    assert_eq!(shortcut_to_hyprland_modmask(all_mods), 64 | 4 | 8 | 1); // 77
}

#[test]
fn matches_hyprland_keys_case_insensitively_and_named() {
    // Character matching matches both upper and lower case
    assert!(matches_hyprland_key("V", ShortcutKey::Character('v')));
    assert!(matches_hyprland_key("v", ShortcutKey::Character('v')));
    assert!(matches_hyprland_key("V", ShortcutKey::Character('V')));
    assert!(!matches_hyprland_key("C", ShortcutKey::Character('v')));

    // Named keys
    assert!(matches_hyprland_key(
        "space",
        ShortcutKey::Named(NamedKey::Space)
    ));
    assert!(matches_hyprland_key(
        "SPACE",
        ShortcutKey::Named(NamedKey::Space)
    ));
    assert!(matches_hyprland_key(
        "F12",
        ShortcutKey::Named(NamedKey::F(12))
    ));
    assert!(matches_hyprland_key(
        "f12",
        ShortcutKey::Named(NamedKey::F(12))
    ));
    assert!(matches_hyprland_key(
        "Return",
        ShortcutKey::Named(NamedKey::Enter)
    ));
    assert!(matches_hyprland_key(
        "enter",
        ShortcutKey::Named(NamedKey::Enter)
    ));
}

#[test]
fn diagnoses_ipc_binds_verified_direct_exec() {
    let raw_json = r#"[
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "exec",
            "arg": "pookie-paste --toggle",
            "submap": ""
        }
    ]"#;

    let diag = diagnose_hyprland_ipc_binds(raw_json, Shortcut::super_v());
    assert_eq!(diag, HyprlandBindingDiagnosis::VerifiedPookie);
}

#[test]
fn diagnoses_ipc_binds_occupied_opaque_lua() {
    // Exactly matches the ground truth JSON observed on Hyprland 0.56.2
    let raw_json = r#"[
        {
            "locked": false,
            "mouse": false,
            "release": false,
            "repeat": false,
            "longPress": false,
            "non_consuming": false,
            "auto_consuming": false,
            "has_description": false,
            "modmask": 64,
            "submap": "",
            "submap_universal": "false",
            "key": "V",
            "keycode": 0,
            "catch_all": false,
            "description": "",
            "allow_input_capture": false,
            "dispatcher": "__lua",
            "arg": "99"
        }
    ]"#;

    let diag = diagnose_hyprland_ipc_binds(raw_json, Shortcut::super_v());
    assert_eq!(
        diag,
        HyprlandBindingDiagnosis::OccupiedOpaque {
            callback_id: "99".to_string()
        }
    );
}

#[test]
fn diagnoses_ipc_binds_definite_conflict_for_competing_exec_and_dispatchers() {
    // 1. Competing exec
    let raw_json1 = r#"[
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "exec",
            "arg": "cliphist list | rofi -dmenu",
            "submap": ""
        }
    ]"#;
    assert_eq!(
        diagnose_hyprland_ipc_binds(raw_json1, Shortcut::super_v()),
        HyprlandBindingDiagnosis::Conflict {
            command: "exec cliphist list | rofi -dmenu".to_string()
        }
    );

    // 2. Competing non-exec dispatcher (window.close, killactive, etc.)
    let raw_json2 = r#"[
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "killactive",
            "arg": "",
            "submap": ""
        }
    ]"#;
    assert_eq!(
        diagnose_hyprland_ipc_binds(raw_json2, Shortcut::super_v()),
        HyprlandBindingDiagnosis::Conflict {
            command: "killactive".to_string()
        }
    );
}

#[test]
fn diagnoses_ipc_binds_deterministic_precedence_rule() {
    // Multiple entries matching the same key/modmask in default submap:
    // If one is a conflict and one is __lua or exec, the conflict MUST take precedence!
    let raw_json_conflict_first = r#"[
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "__lua",
            "arg": "99",
            "submap": ""
        },
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "killactive",
            "arg": "",
            "submap": ""
        }
    ]"#;

    assert!(matches!(
        diagnose_hyprland_ipc_binds(raw_json_conflict_first, Shortcut::super_v()),
        HyprlandBindingDiagnosis::Conflict { .. }
    ));

    // If one is verified exec and one is __lua (without conflict), verified exec takes precedence
    let raw_json_verified = r#"[
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "__lua",
            "arg": "99",
            "submap": ""
        },
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "exec",
            "arg": "pookie-paste --toggle",
            "submap": ""
        }
    ]"#;

    assert_eq!(
        diagnose_hyprland_ipc_binds(raw_json_verified, Shortcut::super_v()),
        HyprlandBindingDiagnosis::VerifiedPookie
    );
}

#[test]
fn diagnoses_ipc_binds_prioritizes_default_submap() {
    // Key matches in a named submap ("passthrough"), but not in default submap ("")
    let raw_json_submap = r#"[
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "exec",
            "arg": "pookie-paste --toggle",
            "submap": "passthrough"
        }
    ]"#;

    assert_eq!(
        diagnose_hyprland_ipc_binds(raw_json_submap, Shortcut::super_v()),
        HyprlandBindingDiagnosis::NotFound
    );
}

#[test]
fn diagnoses_static_hyprland_lua_narrow_literal() {
    let super_v = Shortcut::super_v();

    // 1. Literal match
    let lua_content1 = r#"
        -- User configuration
        hl.bind("SUPER + V", hl.dsp.exec_cmd("pookie-paste --toggle"))
    "#;
    assert_eq!(
        diagnose_hyprland_config_text(lua_content1, super_v),
        HyprlandBindingDiagnosis::VerifiedPookie
    );

    // 2. Literal conflict
    let lua_content2 = r#"
        hl.bind("SUPER + V", hl.dsp.exec_cmd("wl-copy conflict"))
    "#;
    assert!(matches!(
        diagnose_hyprland_config_text(lua_content2, super_v),
        HyprlandBindingDiagnosis::Conflict { .. }
    ));

    // 3. Dynamic concatenation / variables remain unknown without evaluating Lua
    let lua_dynamic = r#"
        hl.bind(mainMod .. " + V", hl.dsp.exec_cmd(command))
    "#;
    assert_eq!(
        diagnose_hyprland_config_text(lua_dynamic, super_v),
        HyprlandBindingDiagnosis::NotFound
    );
}

#[test]
fn diagnoses_static_hyprlang_conf() {
    let super_v = Shortcut::super_v();

    // 1. Classic bind with variable substitution
    let conf_content = r#"
        $mainMod = SUPER
        bind = $mainMod, V, exec, pookie-paste --toggle
        bind = $mainMod, Q, killactive,
    "#;
    assert_eq!(
        diagnose_hyprland_config_text(conf_content, super_v),
        HyprlandBindingDiagnosis::VerifiedPookie
    );

    // 2. Conflict
    let conf_conflict = r#"
        bind = SUPER, V, exec, cliphist list
    "#;
    assert_eq!(
        diagnose_hyprland_config_text(conf_conflict, super_v),
        HyprlandBindingDiagnosis::Conflict {
            command: "exec cliphist list".to_string()
        }
    );
}

#[test]
fn backend_lifecycle_and_mock_ipc_outcomes() {
    // 1. Mock IPC with opaque Lua binding
    let lua_mock_json = r#"[
        {
            "modmask": 64,
            "key": "V",
            "dispatcher": "__lua",
            "arg": "99",
            "submap": ""
        }
    ]"#;

    let mut backend = HyprlandShortcutBackend::with_mock_ipc(lua_mock_json);
    assert_eq!(backend.name(), "Hyprland compositor-managed shortcut");
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
            diagnostic,
        } => {
            assert_eq!(
                binding_snippet,
                "hl.bind(\"SUPER + V\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
            );
            // Crucial: Opaque Lua binding MUST NOT be reported as verified=true
            assert!(!verified);
            // Crucial: Opaque Lua binding MUST NOT be reported as a conflict
            assert!(conflict.is_none());
            // Diagnostic MUST explain the opaque Lua callback state
            assert!(diagnostic.is_some());
            let diag_str = diagnostic.unwrap();
            assert!(diag_str.contains("__lua"));
            assert!(diag_str.contains("99"));
        }
        other => panic!("expected CompositorManaged outcome, got: {other:?}"),
    }

    assert_eq!(backend.registered_shortcut(), Some(shortcut));

    // CompositorManaged wait_for_activation cleanly returns Unavailable
    assert!(matches!(
        backend.wait_for_activation(),
        Err(ShortcutError::Unavailable)
    ));

    backend.unregister().expect("clean unregister");
    assert_eq!(backend.registered_shortcut(), None);
}

#[test]
fn offline_hyprland_backend_deterministic_isolation() {
    // 1. Completely offline without config file -> NotConfigured
    let mut backend = HyprlandShortcutBackend::from_offline_config(None);
    let outcome = backend
        .register(Shortcut::super_v())
        .expect("registration succeeds");

    match outcome {
        ShortcutRegistrationOutcome::CompositorManaged {
            binding_snippet,
            verified,
            conflict,
            diagnostic,
        } => {
            assert_eq!(
                binding_snippet,
                "hl.bind(\"SUPER + V\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
            );
            assert!(!verified);
            assert!(conflict.is_none());
            assert!(diagnostic.is_none());
        }
        other => panic!("expected CompositorManaged outcome, got: {other:?}"),
    }

    // 2. Offline with static Pookie config -> verified remains false because IPC was offline
    let static_pookie = "bind = SUPER, V, exec, pookie-paste --toggle";
    let mut backend_pookie = HyprlandShortcutBackend::from_offline_config(Some(static_pookie));
    let outcome_pookie = backend_pookie
        .register(Shortcut::super_v())
        .expect("registration succeeds");

    match outcome_pookie {
        ShortcutRegistrationOutcome::CompositorManaged {
            verified,
            conflict,
            diagnostic,
            ..
        } => {
            // Must NOT report verified=true when live IPC was offline!
            assert!(!verified);
            assert!(conflict.is_none());
            assert!(diagnostic.is_some());
        }
        other => panic!("expected CompositorManaged outcome, got: {other:?}"),
    }
}
