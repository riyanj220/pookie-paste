use pookie_clipboard::{ClipboardBackend, x11::X11Clipboard};

fn has_display() -> bool {
    std::env::var("DISPLAY").is_ok()
}

#[test]
fn test_x11_clipboard_read() {
    if !has_display() {
        return;
    }

    let clipboard = X11Clipboard::new().expect("Failed to initialize clipboard");

    assert!(clipboard.read().is_ok());
}

#[test]
fn test_x11_clipboard_write() {
    if !has_display() {
        return;
    }

    let clipboard = X11Clipboard::new().expect("Failed to initialize clipboard");

    let result = clipboard.write("Pookie Paste Test");

    assert!(result.is_ok());
}
