use pookie_clipboard::{ClipboardBackend, x11::X11Clipboard};

fn is_x11_session() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    session_type == "x11"
}

#[test]
fn test_x11_clipboard_read() {
    if !is_x11_session() {
        return;
    }

    let clipboard = X11Clipboard::new().expect("Failed to initialize clipboard");

    assert!(clipboard.read().is_ok());
}

#[test]
fn test_x11_clipboard_write() {
    if !is_x11_session() {
        return;
    }

    let clipboard = X11Clipboard::new().expect("Failed to initialize clipboard");

    let result = clipboard.write("Pookie Paste Test");

    assert!(result.is_ok());
}
