use std::time::Duration;

use daemon::paste_backend::PasteBackend;
use daemon::wlroots_paste_backend::WlrootsPasteBackend;

fn main() -> anyhow::Result<()> {
    println!("wlroots virtual keyboard paste backend probe");
    println!("--------------------------------------------");
    println!();

    let is_sway = std::env::var_os("SWAYSOCK").is_some();
    let is_hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();

    if is_sway {
        println!("Detected compositor: Sway ($SWAYSOCK present)");
    } else if is_hyprland {
        println!("Detected compositor: Hyprland ($HYPRLAND_INSTANCE_SIGNATURE present)");
    } else {
        println!("Note: Neither SWAYSOCK nor HYPRLAND_INSTANCE_SIGNATURE detected.");
    }

    let backend = WlrootsPasteBackend::new().map_err(|error| {
        anyhow::anyhow!(
            "failed to initialize WlrootsPasteBackend: {error:?} (is WAYLAND_DISPLAY set and zwp_virtual_keyboard_v1 supported?)"
        )
    })?;

    println!("OK: WlrootsPasteBackend initialized successfully");
    println!("Backend name: {}", backend.name());
    println!();
    println!("Copy some text to your clipboard first.");
    println!(
        "Keep the window where you want to test paste focused (e.g. this Terminal or a text editor)."
    );
    println!("Paste (Ctrl+V) will trigger in 5 seconds...");
    println!();

    std::thread::sleep(Duration::from_secs(5));

    println!("Sending Ctrl+V via zwp_virtual_keyboard_v1...");
    backend
        .paste()
        .map_err(|error| anyhow::anyhow!("paste execution failed: {error:?}"))?;

    println!();
    println!("OK: virtual keyboard simulated Ctrl+V keystrokes");
    println!("Check your focused application to verify the clipboard contents were inserted.");
    println!();
    println!("Wlroots production paste probe: PASS");

    Ok(())
}
