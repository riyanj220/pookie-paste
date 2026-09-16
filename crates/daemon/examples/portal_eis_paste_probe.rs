use std::time::Duration;

use daemon::{paste_backend::PasteBackend, portal_eis_paste_backend::PortalEisPasteBackend};

fn main() -> anyhow::Result<()> {
    println!("Pookie production Portal/EIS paste probe");
    println!("----------------------------------------");
    println!();

    println!("Initializing PortalEisPasteBackend...");

    let backend = PortalEisPasteBackend::new().map_err(|error| {
        anyhow::anyhow!("failed to initialize production Portal/EIS backend: {error:?}")
    })?;

    println!("OK: production Portal/EIS backend initialized");
    println!();
    println!("Before continuing:");
    println!("1. Copy some text normally.");
    println!("2. Focus Kate/KWrite.");
    println!("3. Do not press Ctrl+V yourself.");
    println!();
    println!("Automatic Ctrl+V will be sent in 5 seconds.");

    for remaining in (1..=5).rev() {
        println!("{remaining}...");

        std::thread::sleep(Duration::from_secs(1));
    }

    println!();
    println!("Calling production paste()...");

    backend
        .paste()
        .map_err(|error| anyhow::anyhow!("production paste() failed: {error:?}"))?;

    println!("OK: paste() returned successfully");
    println!();
    println!("Check Kate/KWrite.");
    println!("If the copied text appeared, production EIS injection works.");
    println!("If nothing appeared, the bug is inside PortalEisPasteBackend.");

    Ok(())
}
