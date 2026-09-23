use std::io::Write;
use std::os::fd::AsFd;
use std::sync::Mutex;
use std::time::Duration;

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
};

use crate::paste_backend::{PasteBackend, PasteCapability, PasteError};

/// Linux evdev keycode for Left Control (from linux/input-event-codes.h).
pub const KEY_LEFTCTRL: u32 = 29;

/// Linux evdev keycode for 'V' (from linux/input-event-codes.h).
pub const KEY_V: u32 = 47;

pub const KEY_PRESS: u32 = 1;
pub const KEY_RELEASE: u32 = 0;

pub const KEY_INTERVAL: Duration = Duration::from_millis(25);

/// Canonical complete XKB keymap string for virtual keyboard key injection.
///
/// - "evdev+aliases(qwerty)" maps Linux evdev scancode 29 to `<LCTL>` and 47 to `<AB04>`.
/// - "complete" sets up standard modifier types and compatibility maps.
/// - "pc+us+inet(evdev)" maps `<LCTL>` to `Control_L` and `<AB04>` to `v` / `V`.
///
/// The compositor's internal `libxkbcommon` parses this keymap and maps events to Ctrl+V.
pub const XKB_KEYMAP_STRING: &str = r#"xkb_keymap {
    xkb_keycodes  { include "evdev+aliases(qwerty)" };
    xkb_types     { include "complete" };
    xkb_compat    { include "complete" };
    xkb_symbols   { include "pc+us+inet(evdev)" };
};
"#;

#[derive(Debug, Default)]
pub struct WlrootsState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WlrootsState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WlrootsState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1, ()> for WlrootsState {
    fn event(
        _state: &mut Self,
        _proxy: &zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
        _event: zwp_virtual_keyboard_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, ()> for WlrootsState {
    fn event(
        _state: &mut Self,
        _proxy: &zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
        _event: zwp_virtual_keyboard_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

pub struct WlrootsPasteBackend {
    connection: Connection,
    event_queue: Mutex<EventQueue<WlrootsState>>,
    virtual_keyboard: zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
}

impl WlrootsPasteBackend {
    /// Connects to the Wayland display, binds the virtual keyboard manager on the seat,
    /// and initializes the XKB keymap for virtual input.
    pub fn new() -> Result<Self, PasteError> {
        let connection = Connection::connect_to_env().map_err(|error| {
            PasteError::Failed(format!("failed connecting to Wayland: {error}"))
        })?;

        let (globals, mut event_queue) =
            registry_queue_init::<WlrootsState>(&connection).map_err(|error| {
                PasteError::Failed(format!(
                    "failed initializing Wayland registry queue: {error}"
                ))
            })?;

        let qh = event_queue.handle();

        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=10, ())
            .map_err(|_| PasteError::Unavailable)?;

        let manager = globals
            .bind::<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, _, _>(
                &qh,
                1..=1,
                (),
            )
            .map_err(|_| PasteError::Unavailable)?;

        let virtual_keyboard = manager.create_virtual_keyboard(&seat, &qh, ());

        let keymap_file = create_keymap_memfd(XKB_KEYMAP_STRING)?;
        let keymap_size = XKB_KEYMAP_STRING.len() as u32;

        // format 1 = WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1
        virtual_keyboard.keymap(1, keymap_file.as_fd(), keymap_size);

        let mut state = WlrootsState;
        event_queue.roundtrip(&mut state).map_err(|error| {
            PasteError::Failed(format!("failed initial Wayland roundtrip: {error}"))
        })?;

        Ok(Self {
            connection,
            event_queue: Mutex::new(event_queue),
            virtual_keyboard,
        })
    }

    pub fn name(&self) -> &'static str {
        "Wayland wlroots virtual keyboard direct paste"
    }
}

impl PasteBackend for WlrootsPasteBackend {
    fn capability(&self) -> PasteCapability {
        PasteCapability::Direct
    }

    fn paste(&self) -> Result<(), PasteError> {
        // 1. Press Left Ctrl
        self.virtual_keyboard.key(0, KEY_LEFTCTRL, KEY_PRESS);
        // 2. Press V
        self.virtual_keyboard.key(0, KEY_V, KEY_PRESS);
        self.connection.flush().map_err(|error| {
            PasteError::Failed(format!("failed flushing Wayland key press events: {error}"))
        })?;

        // Allow target window to register key down
        std::thread::sleep(KEY_INTERVAL);

        // 3. Release V
        self.virtual_keyboard.key(0, KEY_V, KEY_RELEASE);
        // 4. Release Left Ctrl
        self.virtual_keyboard.key(0, KEY_LEFTCTRL, KEY_RELEASE);
        self.connection.flush().map_err(|error| {
            PasteError::Failed(format!(
                "failed flushing Wayland key release events: {error}"
            ))
        })?;

        // Roundtrip to ensure compositor processed events
        if let Ok(mut queue) = self.event_queue.lock() {
            let mut state = WlrootsState;
            let _ = queue.roundtrip(&mut state);
        }

        Ok(())
    }

    fn shutdown(&self) {
        self.virtual_keyboard.destroy();
        let _ = self.connection.flush();
    }
}

/// Creates a memory-backed anonymous file descriptor populated with the XKB keymap string.
pub fn create_keymap_memfd(keymap_str: &str) -> Result<std::fs::File, PasteError> {
    let owned_fd = rustix::fs::memfd_create(
        "pookie-paste-keymap",
        rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
    )
    .map_err(|error| PasteError::Failed(format!("failed creating memfd for keymap: {error}")))?;

    let mut file = std::fs::File::from(owned_fd);
    file.write_all(keymap_str.as_bytes())
        .map_err(|error| PasteError::Failed(format!("failed writing keymap to memfd: {error}")))?;
    file.flush()
        .map_err(|error| PasteError::Failed(format!("failed flushing keymap memfd: {error}")))?;

    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycodes_match_linux_evdev_standard() {
        assert_eq!(KEY_LEFTCTRL, 29);
        assert_eq!(KEY_V, 47);
        assert_eq!(KEY_PRESS, 1);
        assert_eq!(KEY_RELEASE, 0);
    }

    #[test]
    fn creates_valid_keymap_memfd() {
        let file = create_keymap_memfd(XKB_KEYMAP_STRING).expect("memfd creation failed");
        let metadata = file.metadata().expect("failed reading memfd metadata");
        assert_eq!(metadata.len(), XKB_KEYMAP_STRING.len() as u64);
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
}
