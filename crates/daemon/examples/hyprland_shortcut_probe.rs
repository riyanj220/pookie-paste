//! Hyprland Shortcut Integration Verification Probe (Phase 14.4)
//!
//! Evaluates live Hyprland IPC `j/binds` querying, modmask calculation, key matching,
//! opaque Lua callback detection, and static configuration diagnostics.
//!
//! Usage:
//!   cargo run -p daemon --example hyprland_shortcut_probe

use std::path::PathBuf;

use daemon::hyprland_shortcut_backend::{
    HyprlandShortcutBackend, format_hyprland_hyprlang_binding, format_hyprland_lua_binding,
};
use daemon::shortcut_backend::{
    NamedKey, Shortcut, ShortcutBackend, ShortcutKey, ShortcutModifiers,
    ShortcutRegistrationOutcome,
};

fn main() -> anyhow::Result<()> {
    println!("=== Hyprland Shortcut Integration Probe (Phase 14.4) ===");
    println!();

    println!("Environment:");
    println!(
        "  XDG_SESSION_TYPE:            {:?}",
        std::env::var("XDG_SESSION_TYPE").ok()
    );
    println!(
        "  XDG_CURRENT_DESKTOP:         {:?}",
        std::env::var("XDG_CURRENT_DESKTOP").ok()
    );
    println!(
        "  HYPRLAND_INSTANCE_SIGNATURE: {:?}",
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()
    );
    println!(
        "  XDG_RUNTIME_DIR:             {:?}",
        std::env::var("XDG_RUNTIME_DIR").ok()
    );

    let sig_opt = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
    let is_hyprland_active = sig_opt.as_ref().is_some_and(|sig| {
        let candidate = std::env::var("XDG_RUNTIME_DIR")
            .ok()
            .map(|xdg| {
                PathBuf::from(xdg)
                    .join("hypr")
                    .join(sig)
                    .join(".socket.sock")
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/hypr").join(sig).join(".socket.sock"));
        candidate.exists()
    });

    if !is_hyprland_active {
        println!();
        println!("NOTE: Hyprland command socket (.socket.sock) is not active in this session.");
        println!("Running diagnostic test in offline mode (using config file inspection)...");
    }
    println!();

    let mut backend = HyprlandShortcutBackend::new()
        .map_err(|e| anyhow::anyhow!("failed to initialize HyprlandShortcutBackend: {e:?}"))?;

    println!("Backend:    {}", backend.name());
    println!("Capability: {:?}", backend.capability());
    if let Some(sock) = backend.socket_path() {
        println!("IPC Socket: {}", sock.display());
    } else {
        println!("IPC Socket: (offline / disconnected)");
    }
    println!();

    // 1. Primary Shortcut Diagnostics (Super+V)
    let primary = Shortcut::super_v();
    test_shortcut(&mut backend, primary, "1. Primary Shortcut (Super+V)")?;

    // 2. Custom Alternate Shortcut Diagnostics (Ctrl+Shift+P)
    let alternate = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    test_shortcut(
        &mut backend,
        alternate,
        "2. Custom Alternate Shortcut (Ctrl+Shift+P)",
    )?;

    // 3. Test Fixture (Super+F12)
    let f12_shortcut = Shortcut::new(
        ShortcutKey::Named(NamedKey::F(12)),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    test_shortcut(
        &mut backend,
        f12_shortcut,
        "3. Named Function Key (Super+F12)",
    )?;

    // 4. Intentional Conflict Key (Super+C)
    let conflict_shortcut = Shortcut::new(
        ShortcutKey::Character('c'),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    test_shortcut(
        &mut backend,
        conflict_shortcut,
        "4. Competing / Conflict Key (Super+C)",
    )?;

    // 5. Unconfigured Shortcut (Ctrl+Shift+A)
    let unconfigured_shortcut = Shortcut::new(
        ShortcutKey::Character('a'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    test_shortcut(
        &mut backend,
        unconfigured_shortcut,
        "5. Unconfigured Key (Ctrl+Shift+A)",
    )?;

    // 6. Clean Unregistration Lifecycle
    println!("6. Testing clean unregistration...");
    backend
        .unregister()
        .map_err(|e| anyhow::anyhow!("unregister failed: {e:?}"))?;
    assert_eq!(backend.registered_shortcut(), None);
    println!("   OK: backend reset successfully");

    println!();
    println!("Hyprland Shortcut Production Probe: PASS");
    Ok(())
}

fn test_shortcut(
    backend: &mut HyprlandShortcutBackend,
    shortcut: Shortcut,
    label: &str,
) -> anyhow::Result<()> {
    println!("{label}: {shortcut}");
    let outcome = backend
        .register(shortcut)
        .map_err(|e| anyhow::anyhow!("registration failed for {shortcut}: {e:?}"))?;

    match outcome {
        ShortcutRegistrationOutcome::CompositorManaged {
            binding_snippet,
            verified,
            conflict,
            diagnostic,
        } => {
            println!("   Generated directive (Lua): {binding_snippet}");
            if verified {
                println!("   Status: VERIFIED (active direct exec in running Hyprland compositor)");
            } else if let Some(conflict_details) = conflict {
                println!("   Status: CONFLICT DETECTED!");
                println!("   Conflict:   {conflict_details}");
            } else if let Some(diag_info) = diagnostic {
                println!("   Status: OCCUPIED (Opaque Lua Callback)");
                println!("   Diagnostic: {diag_info}");
            } else {
                println!("   Status: NOT CONFIGURED");
                println!("   Action required: Add one of the following to your configuration:");
                println!("     For ~/.config/hypr/hyprland.lua (modern Lua):");
                println!("       {}", format_hyprland_lua_binding(shortcut));
                println!("     For ~/.config/hypr/hyprland.conf (classic Hyprlang):");
                println!("       {}", format_hyprland_hyprlang_binding(shortcut));
            }
        }
        other => anyhow::bail!("expected CompositorManaged outcome, got: {other:?}"),
    }
    println!();
    Ok(())
}
