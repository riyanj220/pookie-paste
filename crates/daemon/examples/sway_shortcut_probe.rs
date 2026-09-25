use std::path::PathBuf;

use daemon::shortcut_backend::{
    NamedKey, Shortcut, ShortcutBackend, ShortcutKey, ShortcutModifiers,
    ShortcutRegistrationOutcome,
};
use daemon::sway_shortcut_backend::SwayShortcutBackend;

fn main() -> anyhow::Result<()> {
    println!("Sway Shortcut Backend Production Probe");
    println!("======================================");
    println!();

    let sock_env = std::env::var_os("SWAYSOCK");
    let is_sway_running = sock_env
        .as_ref()
        .map(PathBuf::from)
        .map(|p| p.exists())
        .unwrap_or(false);

    if !is_sway_running {
        println!("NOTE: Sway IPC socket ($SWAYSOCK) is not active in this session.");
        println!("Running diagnostic test in offline mode (using config file inspection)...");
        println!();
    }

    let mut backend = SwayShortcutBackend::new()
        .map_err(|e| anyhow::anyhow!("failed to initialize SwayShortcutBackend: {e:?}"))?;

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
    println!("1. Evaluating primary shortcut ({primary})...");
    let outcome = backend
        .register(primary)
        .map_err(|e| anyhow::anyhow!("registration failed for {primary}: {e:?}"))?;

    match outcome {
        ShortcutRegistrationOutcome::CompositorManaged {
            binding_snippet,
            verified,
            conflict,
            diagnostic: _,
        } => {
            println!("   Generated directive: {binding_snippet}");
            if verified {
                println!("   Status: VERIFIED (active in running Sway compositor)");
            } else if let Some(conflict_details) = conflict {
                println!("   Status: CONFLICT DETECTED!");
                println!("   Conflict: {conflict_details}");
            } else {
                println!("   Status: NOT CONFIGURED");
                println!(
                    "   Action required: Add the following line to ~/.config/sway/config and reload Sway ($mod+Shift+c):"
                );
                println!("     {binding_snippet}");
            }
        }
        other => anyhow::bail!("expected CompositorManaged outcome, got: {other:?}"),
    }
    println!();

    // 2. Custom Alternate Shortcut Diagnostics (Ctrl+Shift+P)
    let alternate = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            control: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    println!("2. Evaluating alternate shortcut ({alternate})...");
    let alt_outcome = backend
        .register(alternate)
        .map_err(|e| anyhow::anyhow!("registration failed for {alternate}: {e:?}"))?;

    match alt_outcome {
        ShortcutRegistrationOutcome::CompositorManaged {
            binding_snippet,
            verified,
            conflict,
            diagnostic: _,
        } => {
            println!("   Generated directive: {binding_snippet}");
            if verified {
                println!("   Status: VERIFIED (active in running Sway compositor)");
            } else if let Some(conflict_details) = conflict {
                println!("   Status: CONFLICT DETECTED: {conflict_details}");
            } else {
                println!("   Status: NOT CONFIGURED");
            }
        }
        other => anyhow::bail!("expected CompositorManaged outcome, got: {other:?}"),
    }
    println!();

    // 3. Named Key Diagnostics (Super+Space)
    let space_shortcut = Shortcut::new(
        ShortcutKey::Named(NamedKey::Space),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );
    println!("3. Evaluating named key shortcut ({space_shortcut})...");
    let space_outcome = backend
        .register(space_shortcut)
        .map_err(|e| anyhow::anyhow!("registration failed for {space_shortcut}: {e:?}"))?;

    match space_outcome {
        ShortcutRegistrationOutcome::CompositorManaged {
            binding_snippet,
            verified,
            conflict,
            diagnostic: _,
        } => {
            println!("   Generated directive: {binding_snippet}");
            if verified {
                println!("   Status: VERIFIED");
            } else if let Some(conflict_details) = conflict {
                println!("   Status: CONFLICT DETECTED: {conflict_details}");
            } else {
                println!("   Status: NOT CONFIGURED");
            }
        }
        other => anyhow::bail!("expected CompositorManaged outcome, got: {other:?}"),
    }
    println!();

    // 4. Clean Unregistration
    println!("4. Testing clean unregistration...");
    backend
        .unregister()
        .map_err(|e| anyhow::anyhow!("unregister failed: {e:?}"))?;
    assert_eq!(backend.registered_shortcut(), None);
    println!("   OK: backend reset successfully");

    println!();
    println!("Sway Shortcut Production Probe: PASS");
    Ok(())
}
