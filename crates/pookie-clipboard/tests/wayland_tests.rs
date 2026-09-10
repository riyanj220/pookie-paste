use pookie_clipboard::WaylandClipboardWatcher;

fn has_wayland_display() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

#[test]
fn test_wayland_watcher_initialization() {
    if !has_wayland_display() {
        return;
    }

    let watcher = WaylandClipboardWatcher::new();

    assert!(
        watcher.is_ok(),
        "Failed to initialize Wayland clipboard watcher"
    );
}
