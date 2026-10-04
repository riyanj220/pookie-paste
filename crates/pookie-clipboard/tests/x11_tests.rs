use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pookie_clipboard::{
    ClipboardBackend, ClipboardContent, ClipboardEvent, ClipboardWatcher, canonicalize_rgba,
    decode_canonical_png_to_rgba, x11::X11Clipboard, x11_watcher::X11ClipboardWatcher,
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

    let (width, height, actual_rgba) = decode_canonical_png_to_rgba(actual_png.png_bytes())
        .expect("failed decoding X11 round-trip image");

    assert_eq!(width, 2);
    assert_eq!(height, 2);
    assert_eq!(actual_rgba, rgba);
}

const BOUNDED_WAIT_TIMEOUT: Duration = Duration::from_secs(3);

fn recv_event_bounded(
    rx: &mut tokio::sync::mpsc::Receiver<ClipboardEvent>,
    timeout: Duration,
) -> ClipboardEvent {
    let start = std::time::Instant::now();
    loop {
        match rx.try_recv() {
            Ok(event) => return event,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                panic!("channel disconnected unexpectedly while waiting for clipboard event");
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                if start.elapsed() >= timeout {
                    panic!("timed out after {timeout:?} waiting for clipboard event");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn wait_for_channel_closed_bounded(
    rx: &mut tokio::sync::mpsc::Receiver<ClipboardEvent>,
    timeout: Duration,
) {
    let start = std::time::Instant::now();
    loop {
        match rx.try_recv() {
            Ok(event) => {
                tracing::debug!(?event, "drained leftover event during shutdown wait");
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                if start.elapsed() >= timeout {
                    panic!("timed out after {timeout:?} waiting for watcher channel to close");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn test_x11_watcher_startup_emits_existing_content() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = Arc::new(X11Clipboard::new().expect("failed to initialize clipboard"));
    clipboard
        .write("Pookie Watcher Startup Text")
        .expect("failed writing startup text");

    let mut watcher = X11ClipboardWatcher::new(clipboard);
    let mut rx = watcher.start();

    let event = recv_event_bounded(&mut rx, BOUNDED_WAIT_TIMEOUT);

    assert_eq!(
        event.content,
        ClipboardContent::Text("Pookie Watcher Startup Text".to_string())
    );

    watcher.shutdown();
}

#[test]
fn test_x11_watcher_recopy_same_text_emits_event() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = Arc::new(X11Clipboard::new().expect("failed to initialize clipboard"));
    clipboard
        .write("Pookie Recopy Text")
        .expect("failed writing initial text");

    let mut watcher = X11ClipboardWatcher::new(Arc::clone(&clipboard));
    let mut rx = watcher.start();

    // Consume initial startup event
    let event1 = recv_event_bounded(&mut rx, BOUNDED_WAIT_TIMEOUT);
    assert_eq!(
        event1.content,
        ClipboardContent::Text("Pookie Recopy Text".to_string())
    );

    // Intentionally recopy the exact same text
    std::thread::sleep(Duration::from_millis(50));
    clipboard
        .write("Pookie Recopy Text")
        .expect("failed recopying same text");

    // Must emit a second event (previous_content suppression removed)
    let event2 = recv_event_bounded(&mut rx, BOUNDED_WAIT_TIMEOUT);
    assert_eq!(
        event2.content,
        ClipboardContent::Text("Pookie Recopy Text".to_string())
    );

    watcher.shutdown();
}

#[test]
fn test_x11_watcher_recopy_same_image_emits_event() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = Arc::new(X11Clipboard::new().expect("failed to initialize clipboard"));
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let canonical = canonicalize_rgba(2, 2, &rgba).expect("failed creating canonical PNG");

    clipboard
        .write_content(&ClipboardContent::Image(canonical.clone()))
        .expect("failed writing initial image");

    let mut watcher = X11ClipboardWatcher::new(Arc::clone(&clipboard));
    let mut rx = watcher.start();

    // Consume initial startup event
    let event1 = recv_event_bounded(&mut rx, BOUNDED_WAIT_TIMEOUT);
    assert_eq!(event1.content, ClipboardContent::Image(canonical.clone()));

    // Intentionally recopy the exact same image
    std::thread::sleep(Duration::from_millis(50));
    clipboard
        .write_content(&ClipboardContent::Image(canonical.clone()))
        .expect("failed recopying same image");

    // Must emit a second event (Problem 2 solved: image recopy emitted)
    let event2 = recv_event_bounded(&mut rx, BOUNDED_WAIT_TIMEOUT);
    assert_eq!(event2.content, ClipboardContent::Image(canonical));

    watcher.shutdown();
}

#[test]
fn test_x11_watcher_clean_shutdown() {
    if !is_x11_session() {
        return;
    }

    let _guard = lock_x11_clipboard_tests();

    let clipboard = Arc::new(X11Clipboard::new().expect("failed to initialize clipboard"));
    let mut watcher = X11ClipboardWatcher::new(clipboard);
    let mut rx = watcher.start();

    // Consume startup event if any
    let _ = rx.try_recv();

    let handle = watcher.shutdown_handle().expect("shutdown handle exists");
    handle.shutdown();

    // Channel should close as worker terminates
    wait_for_channel_closed_bounded(&mut rx, BOUNDED_WAIT_TIMEOUT);

    // Explicit shutdown on watcher must cleanly join worker thread
    watcher.shutdown();
}
