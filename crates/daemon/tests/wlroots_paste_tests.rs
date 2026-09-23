use daemon::paste_backend::{PasteBackend, PasteCapability, PlatformPasteBackend};
use daemon::platform_focus_backend::PlatformFocusBackend;
use daemon::wlroots_paste_backend::{
    KEY_INTERVAL, KEY_LEFTCTRL, KEY_PRESS, KEY_RELEASE, KEY_V, WlrootsPasteBackend,
    XKB_KEYMAP_STRING, create_keymap_memfd,
};

#[test]
fn keycodes_match_linux_evdev_standard() {
    assert_eq!(KEY_LEFTCTRL, 29);
    assert_eq!(KEY_V, 47);
    assert_eq!(KEY_PRESS, 1);
    assert_eq!(KEY_RELEASE, 0);
    assert!(KEY_INTERVAL.as_millis() >= 10);
}

#[test]
fn creates_valid_keymap_memfd_with_expected_content() {
    let file = create_keymap_memfd(XKB_KEYMAP_STRING).expect("memfd creation failed");
    let metadata = file.metadata().expect("failed reading memfd metadata");
    assert_eq!(metadata.len(), XKB_KEYMAP_STRING.len() as u64);

    assert!(XKB_KEYMAP_STRING.contains("evdev+aliases(qwerty)"));
    assert!(XKB_KEYMAP_STRING.contains("pc+us+inet(evdev)"));
    assert!(XKB_KEYMAP_STRING.contains("complete"));
}

#[test]
fn new_fails_cleanly_without_wayland_display() {
    let prev = std::env::var_os("WAYLAND_DISPLAY");
    unsafe { std::env::remove_var("WAYLAND_DISPLAY") };

    let result = WlrootsPasteBackend::new();
    assert!(result.is_err());

    if let Some(val) = prev {
        unsafe { std::env::set_var("WAYLAND_DISPLAY", val) };
    }
}

#[test]
fn fallback_paste_backend_reports_clipboard_only() {
    let backend =
        PlatformPasteBackend::WaylandFallback(daemon::paste_backend::WaylandPasteBackend::new());
    assert_eq!(backend.capability(), PasteCapability::ClipboardOnly);
    assert_eq!(backend.name(), "Wayland clipboard-only");
}

#[test]
fn focus_backend_can_restore_for_sway_and_hyprland() {
    let sway_backend =
        PlatformFocusBackend::Sway(daemon::sway_focus_backend::SwayFocusBackend::from_path(
            std::path::PathBuf::from("/tmp/nonexistent-sway.sock"),
        ));
    assert!(sway_backend.can_restore_focus());

    let hypr_backend = PlatformFocusBackend::Hyprland(
        daemon::hyprland_focus_backend::HyprlandFocusBackend::from_path(std::path::PathBuf::from(
            "/tmp/nonexistent-hypr.sock",
        )),
    );
    assert!(hypr_backend.can_restore_focus());

    let unavail = PlatformFocusBackend::Unavailable(daemon::focus_backend::UnavailableFocusBackend);
    assert!(!unavail.can_restore_focus());
}
