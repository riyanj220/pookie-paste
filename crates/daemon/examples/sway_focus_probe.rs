use std::time::Duration;

use daemon::{focus_backend::FocusBackend, sway_focus_backend::SwayFocusBackend};

fn main() -> anyhow::Result<()> {
    println!("Sway focus backend production probe");
    println!("-----------------------------------");
    println!();

    let backend = SwayFocusBackend::new().map_err(|error| {
        anyhow::anyhow!("failed to initialize Sway focus backend: {error:?} (is SWAYSOCK set?)")
    })?;

    println!(
        "OK: SwayFocusBackend initialized via {}",
        backend.socket_path().display()
    );
    println!();
    println!("Keep the window you want to test focused (e.g. this Terminal or a text editor).");
    println!("Capturing active target in 5 seconds...");

    std::thread::sleep(Duration::from_secs(5));

    let target = backend
        .active_target()
        .map_err(|error| anyhow::anyhow!("failed to capture active target: {error:?}"))?;

    println!("Captured target: {target}");
    println!();
    println!("Now switch to another window (e.g. Firefox or another terminal).");
    println!("Restore will happen in 5 seconds...");

    std::thread::sleep(Duration::from_secs(5));

    backend
        .restore(target.clone())
        .map_err(|error| anyhow::anyhow!("failed to request restore: {error:?}"))?;

    println!("OK: restore requested for {target}");
    println!();
    println!("Waiting for target to become active again...");

    for attempt in 1..=50 {
        if backend
            .is_active(target.clone())
            .map_err(|error| anyhow::anyhow!("failed to query active target: {error:?}"))?
        {
            println!("OK: captured target is active again");
            println!();
            println!("Sway production focus probe: PASS");
            return Ok(());
        }

        println!("Focus check {attempt}: not active yet");
        std::thread::sleep(Duration::from_millis(20));
    }

    anyhow::bail!("captured target was not restored within 1 second");
}
