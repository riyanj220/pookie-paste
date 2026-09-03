use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};

use crate::shortcut_backend::{
    Shortcut, ShortcutBackend, ShortcutError, ShortcutKey, ShortcutModifiers,
};

const XK_NUM_LOCK: u32 = 0xff7f;

pub struct X11ShortcutBackend {
    connection: x11rb::rust_connection::RustConnection,

    root_window: u32,

    num_lock_mask: Option<ModMask>,

    registered_keycode: Option<u8>,

    key_down: bool,

    pending_event: Option<x11rb::protocol::Event>,
}

impl X11ShortcutBackend {
    pub fn new() -> Result<Self, ShortcutError> {
        let (connection, screen_num) = x11rb::connect(None)
            .map_err(|error| ShortcutError::Failed(format!("failed to connect to X11: {error}")))?;

        let root_window = connection
            .setup()
            .roots
            .get(screen_num)
            .ok_or_else(|| {
                ShortcutError::Failed(format!("X11 screen {screen_num} does not exist"))
            })?
            .root;

        let num_lock_mask = find_keycode(&connection, XK_NUM_LOCK)
            .ok()
            .and_then(|keycode| find_modifier_mask(&connection, keycode).ok().flatten());

        Ok(Self {
            connection,
            root_window,
            num_lock_mask,
            registered_keycode: None,
            key_down: false,
            pending_event: None,
        })
    }

    fn next_event(&mut self) -> Result<x11rb::protocol::Event, ShortcutError> {
        if let Some(event) = self.pending_event.take() {
            return Ok(event);
        }

        self.connection.wait_for_event().map_err(|error| {
            ShortcutError::Failed(format!("failed waiting for X11 shortcut: {error}"))
        })
    }
}

impl ShortcutBackend for X11ShortcutBackend {
    fn register(&mut self, shortcut: Shortcut) -> Result<(), ShortcutError> {
        let keysym = shortcut_keysym(shortcut.key)?;

        let keycode = find_keycode(&self.connection, keysym)?;

        let base_mask = x11_modifier_mask(shortcut.modifiers);

        let mut masks = vec![base_mask, base_mask | ModMask::LOCK];

        if let Some(num_lock) = self.num_lock_mask {
            masks.push(base_mask | num_lock);

            masks.push(base_mask | ModMask::LOCK | num_lock);
        }

        for mask in masks {
            self.connection
                .grab_key(
                    false,
                    self.root_window,
                    mask,
                    keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .map_err(|error| {
                    ShortcutError::Failed(format!(
                        "failed to request shortcut registration: {error}"
                    ))
                })?
                .check()
                .map_err(|error| {
                    ShortcutError::Conflict(format!(
                        "shortcut is already in use or could not be registered: {error}"
                    ))
                })?;
        }

        self.connection.flush().map_err(|error| {
            ShortcutError::Failed(format!("failed to flush shortcut registration: {error}"))
        })?;

        self.registered_keycode = Some(keycode);

        self.key_down = false;

        self.pending_event = None;

        Ok(())
    }

    fn wait_for_activation(&mut self) -> Result<(), ShortcutError> {
        let Some(keycode) = self.registered_keycode else {
            return Err(ShortcutError::Failed(
                "shortcut backend has not been registered".to_string(),
            ));
        };

        loop {
            let event = self.next_event()?;

            match event {
                x11rb::protocol::Event::KeyPress(event) if event.detail == keycode => {
                    if self.key_down {
                        continue;
                    }

                    self.key_down = true;

                    return Ok(());
                }

                x11rb::protocol::Event::KeyRelease(release) if release.detail == keycode => {
                    let next_event = self.connection.poll_for_event().map_err(|error| {
                        ShortcutError::Failed(format!("failed checking X11 repeat event: {error}"))
                    })?;

                    if let Some(x11rb::protocol::Event::KeyPress(press)) = next_event {
                        if press.detail == release.detail && press.time == release.time {
                            // Classic X11 auto-repeat:
                            //
                            // KeyRelease + KeyPress with
                            // identical keycode/time.
                            //
                            // The key is still physically
                            // held, so do not re-arm.
                            continue;
                        }

                        self.pending_event = Some(x11rb::protocol::Event::KeyPress(press));
                    } else if let Some(event) = next_event {
                        self.pending_event = Some(event);
                    }

                    self.key_down = false;
                }

                _ => {}
            }
        }
    }
}

fn shortcut_keysym(key: ShortcutKey) -> Result<u32, ShortcutError> {
    match key {
        ShortcutKey::Character(character) if character.is_ascii() => {
            Ok(character.to_ascii_lowercase() as u32)
        }

        ShortcutKey::Character(_) => Err(ShortcutError::Unavailable),
    }
}

fn x11_modifier_mask(modifiers: ShortcutModifiers) -> ModMask {
    let mut mask = ModMask::default();

    if modifiers.super_key {
        mask |= ModMask::M4;
    }

    if modifiers.control {
        mask |= ModMask::CONTROL;
    }

    if modifiers.alt {
        mask |= ModMask::M1;
    }

    if modifiers.shift {
        mask |= ModMask::SHIFT;
    }

    mask
}

fn find_keycode(
    connection: &x11rb::rust_connection::RustConnection,
    keysym: u32,
) -> Result<u8, ShortcutError> {
    let setup = connection.setup();

    let min = setup.min_keycode;

    let max = setup.max_keycode;

    let count = max - min + 1;

    let reply = connection
        .get_keyboard_mapping(min, count)
        .map_err(|error| {
            ShortcutError::Failed(format!("failed to request keyboard mapping: {error}"))
        })?
        .reply()
        .map_err(|error| {
            ShortcutError::Failed(format!("failed to read keyboard mapping: {error}"))
        })?;

    let per_keycode = reply.keysyms_per_keycode as usize;

    for (index, keysyms) in reply.keysyms.chunks(per_keycode).enumerate() {
        if keysyms.contains(&keysym) {
            return Ok(min + index as u8);
        }
    }

    Err(ShortcutError::Failed(format!(
        "could not resolve keysym {keysym:#x}"
    )))
}

fn find_modifier_mask(
    connection: &x11rb::rust_connection::RustConnection,
    keycode: u8,
) -> Result<Option<ModMask>, ShortcutError> {
    let reply = connection
        .get_modifier_mapping()
        .map_err(|error| {
            ShortcutError::Failed(format!("failed to request modifier mapping: {error}"))
        })?
        .reply()
        .map_err(|error| {
            ShortcutError::Failed(format!("failed to read modifier mapping: {error}"))
        })?;

    let per_modifier = reply.keycodes_per_modifier() as usize;

    let modifier_masks = [
        ModMask::SHIFT,
        ModMask::LOCK,
        ModMask::CONTROL,
        ModMask::M1,
        ModMask::M2,
        ModMask::M3,
        ModMask::M4,
        ModMask::M5,
    ];

    for (modifier_index, keycodes) in reply.keycodes.chunks(per_modifier).enumerate() {
        if keycodes.contains(&keycode) {
            return Ok(modifier_masks.get(modifier_index).copied());
        }
    }

    Ok(None)
}
