use pookie_clipboard::{
    ClipboardBackend,
    wayland::{WaylandClipboard, WaylandClipboardWatcher},
};

fn has_wayland_display() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

#[test]
fn test_wayland_watcher_initialization() {
    if !has_wayland_display() {
        return;
    }

    let watcher = WaylandClipboardWatcher::new();

    assert!(
        watcher.is_ok(),
        "failed to initialize Wayland clipboard watcher"
    );
}

#[test]
fn test_wayland_clipboard_write_and_read() {
    if !has_wayland_display() {
        return;
    }

    let clipboard = WaylandClipboard::new();

    let expected = "Pookie Paste Wayland Test";

    clipboard
        .write(expected)
        .expect("failed writing Wayland clipboard");

    let actual = clipboard.read().expect("failed reading Wayland clipboard");

    assert_eq!(
        actual, expected,
        "Wayland clipboard content did not match written content"
    );
}
