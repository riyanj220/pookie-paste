use std::sync::{Mutex, MutexGuard};

use pookie_clipboard::{
    ClipboardBackend, ClipboardContent, canonicalize_rgba, decode_canonical_png_to_rgba,
    wayland::{WaylandClipboard, WaylandClipboardWatcher},
};

static WAYLAND_CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

fn has_wayland_display() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn lock_wayland_clipboard_tests() -> MutexGuard<'static, ()> {
    WAYLAND_CLIPBOARD_TEST_LOCK
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
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

    let _guard = lock_wayland_clipboard_tests();

    let clipboard = WaylandClipboard::new();

    let expected = "Pookie Paste Wayland Test";

    clipboard
    .write(expected)
    .expect("failed writing Wayland clipboard");

    let actual = clipboard
    .read()
    .expect("failed reading Wayland clipboard");

    assert_eq!(
        actual,
        expected,
        "Wayland clipboard content did not match written content",
    );
}

#[test]
fn test_wayland_content_aware_text_round_trip() {
    if !has_wayland_display() {
        return;
    }

    let _guard = lock_wayland_clipboard_tests();

    let clipboard = WaylandClipboard::new();

    let expected =
    ClipboardContent::Text(
        "Pookie Wayland content-aware text".to_string(),
    );

    clipboard
    .write_content(&expected)
    .expect("failed writing content-aware Wayland text");

    let actual = clipboard
    .read_content()
    .expect("failed reading content-aware Wayland text");

    assert_eq!(actual, expected);
}

#[test]
fn test_wayland_image_round_trip() {
    if !has_wayland_display() {
        return;
    }

    let _guard = lock_wayland_clipboard_tests();

    let clipboard = WaylandClipboard::new();

    /*
     * 2x2 RGBA image:
     *
     * red, green
     * blue, white
     */
    let rgba = vec![
        255, 0, 0, 255,
        0, 255, 0, 255,
        0, 0, 255, 255,
        255, 255, 255, 255,
    ];

    let canonical =
    canonicalize_rgba(2, 2, &rgba)
    .expect("failed creating canonical test PNG");

    clipboard
    .write_content(
        &ClipboardContent::Image(canonical),
    )
    .expect("failed writing Wayland image");

    let actual = clipboard
    .read_content()
    .expect("failed reading Wayland image");

    let ClipboardContent::Image(actual_png) = actual else {
        panic!("expected Wayland image clipboard content");
    };

    let (width, height, actual_rgba) =
    decode_canonical_png_to_rgba(&actual_png)
    .expect(
        "failed decoding Wayland round-trip image",
    );

    assert_eq!(width, 2);
    assert_eq!(height, 2);
    assert_eq!(actual_rgba, rgba);
}
