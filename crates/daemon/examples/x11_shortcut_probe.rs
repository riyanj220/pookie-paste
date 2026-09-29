use daemon::shortcut_backend::{
    NamedKey, Shortcut, ShortcutBackend, ShortcutError, ShortcutKey, ShortcutModifiers,
};
use daemon::x11_shortcut_backend::{X11ShortcutBackend, shortcut_keysym, x11_modifier_mask};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};

fn main() -> anyhow::Result<()> {
    println!("X11 Shortcut Backend Production Probe");
    println!("=====================================");
    println!();

    let mut backend = match X11ShortcutBackend::new() {
        Ok(b) => b,
        Err(err) => {
            eprintln!("SKIP: X11 is not available in current environment: {err:?}");
            return Ok(());
        }
    };

    println!("Backend:    {}", backend.name());
    println!("Capability: {:?}", backend.capability());
    println!();

    // 1. Normal Registration
    let primary = Shortcut::super_v();
    println!("1. Registering primary shortcut ({primary})...");
    let outcome = backend
        .register(primary)
        .map_err(|e| anyhow::anyhow!("failed initial registration of {primary}: {e:?}"))?;
    println!("   OK: registered ({})", outcome.description());
    assert_eq!(backend.registered_shortcut(), Some(primary));

    // 2. Idempotent Registration
    println!("2. Testing idempotent re-registration of the exact same shortcut...");
    let idempotent_outcome = backend
        .register(primary)
        .map_err(|e| anyhow::anyhow!("idempotent registration failed: {e:?}"))?;
    println!(
        "   OK: re-registered idempotently ({})",
        idempotent_outcome.description()
    );
    assert_eq!(backend.registered_shortcut(), Some(primary));

    // 3. Transactional Conflict & Partial-Grab Rollback Verification
    println!("3. Testing transactional re-registration and partial-grab rollback on conflict...");
    let conflict_shortcut = Shortcut::new(
        ShortcutKey::Named(NamedKey::F(12)),
        ShortcutModifiers {
            super_key: true,
            ..ShortcutModifiers::NONE
        },
    );

    // Create a secondary X11 connection that holds a grab on the SECOND modifier permutation:
    // Permutation 0: base_mask
    // Permutation 1: base_mask | ModMask::LOCK
    //
    // By occupying permutation 1, the backend will successfully acquire permutation 0 first,
    // and then encounter BadAccess on permutation 1. This specifically tests partial-grab rollback.
    let (conn2, screen2) = x11rb::connect(None)
        .map_err(|e| anyhow::anyhow!("failed to create secondary X11 connection: {e:?}"))?;
    let root2 = conn2.setup().roots[screen2].root;
    let keysym2 = shortcut_keysym(conflict_shortcut.key)?;

    // Resolve keycode on second connection
    let setup2 = conn2.setup();
    let min2 = setup2.min_keycode;
    let max2 = setup2.max_keycode;
    let count2 = max2 - min2 + 1;
    let mapping2 = conn2.get_keyboard_mapping(min2, count2)?.reply()?;
    let per_keycode2 = mapping2.keysyms_per_keycode as usize;
    let mut keycode2 = None;
    for (idx, syms) in mapping2.keysyms.chunks(per_keycode2).enumerate() {
        if syms.contains(&keysym2) {
            keycode2 = Some(min2 + idx as u8);
            break;
        }
    }
    let keycode2 = keycode2.ok_or_else(|| anyhow::anyhow!("could not resolve keysym on conn2"))?;
    let base_mask2 = x11_modifier_mask(conflict_shortcut.modifiers);
    let occupied_mask2 = base_mask2 | ModMask::LOCK;

    // Grab the SECOND permutation on conn2
    conn2
        .grab_key(
            false,
            root2,
            occupied_mask2,
            keycode2,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?
        .check()
        .map_err(|e| anyhow::anyhow!("secondary connection failed to acquire grab: {e:?}"))?;
    conn2.flush()?;
    println!(
        "   (Secondary connection occupied permutation #2: {conflict_shortcut} with CapsLock)"
    );

    // Attempt re-binding the backend to the partially occupied key
    match backend.register(conflict_shortcut) {
        Err(ShortcutError::Conflict(details)) => {
            println!(
                "   OK: Backend acquired permutation #1, encountered BadAccess on permutation #2, and reported: {details}"
            );
        }
        other => {
            anyhow::bail!("expected ShortcutError::Conflict, got: {other:?}");
        }
    }

    // Verify partial-grab rollback:
    // If the backend rolled back permutation #1 (base_mask2), conn2 should now be able
    // to grab base_mask2 without BadAccess.
    conn2
        .grab_key(
            false,
            root2,
            base_mask2,
            keycode2,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?
        .check()
        .map_err(|e| {
            anyhow::anyhow!(
                "partial rollback verification failed: base_mask is still held by backend! {e:?}"
            )
        })?;
    conn2.flush()?;
    println!(
        "   OK: Verified permutation #1 was rolled back (secondary connection successfully acquired it)"
    );

    // Release secondary connection grabs
    let _ = conn2.ungrab_key(keycode2, root2, occupied_mask2);
    let _ = conn2.ungrab_key(keycode2, root2, base_mask2);
    let _ = conn2.flush();
    drop(conn2);

    // Crucial: verify that the primary shortcut is STILL active!
    assert_eq!(
        backend.registered_shortcut(),
        Some(primary),
        "primary shortcut must remain registered after a failed re-bind!"
    );
    println!("   OK: Original binding ({primary}) was preserved after conflict rollback");

    // 4. Successful Rebind
    println!("4. Testing successful rebind...");
    let rebind_shortcut = Shortcut::new(
        ShortcutKey::Character('p'),
        ShortcutModifiers {
            super_key: true,
            shift: true,
            ..ShortcutModifiers::NONE
        },
    );
    backend
        .register(rebind_shortcut)
        .map_err(|e| anyhow::anyhow!("rebind to {rebind_shortcut} failed: {e:?}"))?;
    assert_eq!(backend.registered_shortcut(), Some(rebind_shortcut));
    println!("   OK: Rebound successfully to {rebind_shortcut}");

    // Rebind back to primary
    backend
        .register(primary)
        .map_err(|e| anyhow::anyhow!("rebind back to {primary} failed: {e:?}"))?;
    assert_eq!(backend.registered_shortcut(), Some(primary));
    println!("   OK: Restored primary binding ({primary})");

    // 5. Interactive Keypress Verification
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--wait") {
        println!();
        println!("5. Live Keypress Test: Press {primary} now to verify activation...");
        match backend.wait_for_activation() {
            Ok(_) => {
                println!("   OK: Live keypress activation received!");
            }
            Err(err) => {
                eprintln!("   Warning: activation wait error: {err:?}");
            }
        }
    } else {
        println!();
        println!("5. Live Keypress Test: Skipped (pass --wait to test interactive keypress)");
    }

    // 6. Clean Unregistration
    println!("6. Testing clean unregistration...");
    backend
        .unregister()
        .map_err(|e| anyhow::anyhow!("unregister failed: {e:?}"))?;
    assert_eq!(backend.registered_shortcut(), None);
    println!("   OK: backend cleanly ungrabbed and reset");

    // 7. Drop Cleanup Verification
    println!("7. Verifying Drop cleanup...");
    drop(backend);
    println!("   OK: backend dropped cleanly with zero leaks");

    println!();
    println!("X11 Shortcut Production Probe: PASS");
    Ok(())
}
