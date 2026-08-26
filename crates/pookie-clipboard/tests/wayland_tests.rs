use pookie_clipboard::{ClipboardBackend, wayland::WaylandClipboard};

fn has_wayland_display() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

#[test]
fn test_wayland_clipboard_read() {
    if !has_wayland_display() {
        return;
    }

    let clipboard = WaylandClipboard::new().expect("Failed to initialize Wayland clipboard");

    let result = clipboard.read();

    assert!(result.is_ok());
}

#[test]
fn test_wayland_clipboard_write() {
    if !has_wayland_display() {
        return;
    }

    let clipboard = WaylandClipboard::new().expect("Failed to initialize Wayland clipboard");

    let result = clipboard.write("Pookie Paste Wayland Test");

    assert!(result.is_ok());
}
