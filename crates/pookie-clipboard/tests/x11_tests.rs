use std::sync::{Mutex, MutexGuard};

use pookie_clipboard::{
    ClipboardBackend, ClipboardContent, canonicalize_rgba, decode_canonical_png_to_rgba,
    x11::X11Clipboard,
};

static X11_CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

fn is_x11_session() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    session_type == "x11"
}

fn lock_x11_clipboard_tests() -> MutexGuard<'static, ()> {
    X11_CLIPBOARD_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn test_x11_clipboard_read() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = X11Clipboard::new().expect("failed to initialize clipboard");

    /*
     * Do not depend on whatever happened to be in the user's
     * clipboard before this test started.
     *
     * X11 clipboard contents are owned by a live selection
     * owner, so another test's Clipboard instance may already
     * have been dropped.
     */
    clipboard
        .write("Pookie Paste Read Test")
        .expect("failed preparing X11 clipboard");

    let actual = clipboard.read().expect("failed reading X11 clipboard");

    assert_eq!(actual, "Pookie Paste Read Test");
}

#[test]
fn test_x11_clipboard_write() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = X11Clipboard::new().expect("failed to initialize clipboard");

    let result = clipboard.write("Pookie Paste Test");

    assert!(result.is_ok());
}

#[test]
fn test_x11_content_aware_text_round_trip() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = X11Clipboard::new().expect("failed to initialize clipboard");

    clipboard
        .write_content(&ClipboardContent::Text(
            "Pookie content-aware text".to_string(),
        ))
        .expect("failed writing X11 text");

    let actual = clipboard.read_content().expect("failed reading X11 text");

    assert_eq!(
        actual,
        ClipboardContent::Text("Pookie content-aware text".to_string()),
    );
}

#[test]
fn test_x11_image_round_trip() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = X11Clipboard::new().expect("failed to initialize clipboard");

    /*
     * 2x2 RGBA image:
     *
     * red, green
     * blue, white
     */
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];

    let canonical = canonicalize_rgba(2, 2, &rgba).expect("failed creating canonical PNG");

    clipboard
        .write_content(&ClipboardContent::Image(canonical))
        .expect("failed writing X11 image");

    let actual = clipboard.read_content().expect("failed reading X11 image");

    let ClipboardContent::Image(actual_png) = actual else {
        panic!("expected image clipboard content");
    };

    let (width, height, actual_rgba) =
        decode_canonical_png_to_rgba(&actual_png).expect("failed decoding X11 round-trip image");

    assert_eq!(width, 2);
    assert_eq!(height, 2);
    assert_eq!(actual_rgba, rgba);
}
