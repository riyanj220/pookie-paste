use std::time::Duration;

use daemon::{focus_backend::FocusBackend, kde_focus_backend::KdeFocusBackend};

fn main() -> anyhow::Result<()> {
    println!("KDE focus backend production probe");
    println!("----------------------------------");
    println!();
    println!("Focus Kate/KWrite now.");
    println!("Capturing in 5 seconds...");

    std::thread::sleep(Duration::from_secs(5));

    let backend = KdeFocusBackend::new()
        .map_err(|error| anyhow::anyhow!("failed to initialize KDE focus backend: {error:?}"))?;

    println!("OK: KdeFocusBackend initialized");

    let target = backend
        .active_target()
        .map_err(|error| anyhow::anyhow!("failed to capture active target: {error:?}"))?;

    println!("Captured target: {target}");

    println!();
    println!("Now switch to Terminal or Firefox.");
    println!("Restore will happen in 5 seconds...");

    std::thread::sleep(Duration::from_secs(5));

    backend
        .restore(target.clone())
        .map_err(|error| anyhow::anyhow!("failed to request restore: {error:?}"))?;

    println!("OK: restore requested");

    println!();
    println!("Waiting for target to become active...");

    for attempt in 1..=50 {
        if backend
            .is_active(target.clone())
            .map_err(|error| anyhow::anyhow!("failed to query active target: {error:?}"))?
        {
            println!("OK: captured target is active again");
            println!();
            println!("KDE production focus probe: PASS");
            return Ok(());
        }

        println!("Focus check {attempt}: not active yet");

        std::thread::sleep(Duration::from_millis(20));
    }

    anyhow::bail!("captured target was not restored within 1 second");
}
