use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::ErrorKind;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};

use crate::shortcut_backend::{
    Shortcut, ShortcutActivation, ShortcutBackend, ShortcutBackendCapability, ShortcutError,
    ShortcutKey, ShortcutModifiers, ShortcutRegistrationOutcome,
};

const XK_NUM_LOCK: u32 = 0xff7f;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisteredGrab {
    shortcut: Shortcut,
    keycode: u8,
    masks: Vec<ModMask>,
}

pub struct X11ShortcutBackend {
    connection: x11rb::rust_connection::RustConnection,

    root_window: u32,

    num_lock_mask: Option<ModMask>,

    registered_grab: Option<RegisteredGrab>,

    key_down: bool,

    pending_event: Option<x11rb::protocol::Event>,

    wake_reader: std::os::unix::net::UnixStream,

    wake_writer: std::os::unix::net::UnixStream,
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

        let (wake_reader, wake_writer) = std::os::unix::net::UnixStream::pair().map_err(|e| {
            ShortcutError::Failed(format!("failed to create self-pipe for X11 wake: {e}"))
        })?;
        wake_reader.set_nonblocking(true).map_err(|e| {
            ShortcutError::Failed(format!("failed to set wake_reader nonblocking: {e}"))
        })?;
        wake_writer.set_nonblocking(true).map_err(|e| {
            ShortcutError::Failed(format!("failed to set wake_writer nonblocking: {e}"))
        })?;

        Ok(Self {
            connection,
            root_window,
            num_lock_mask,
            registered_grab: None,
            key_down: false,
            pending_event: None,
            wake_reader,
            wake_writer,
        })
    }

    fn drain_wake_pipe(&self) {
        use std::io::Read;
        let mut buf = [0u8; 64];
        while let Ok(n) = (&self.wake_reader).read(&mut buf) {
            if n == 0 {
                break;
            }
        }
    }

    /// Returns the currently active registered shortcut, if any.
    pub fn registered_shortcut(&self) -> Option<Shortcut> {
        self.registered_grab.as_ref().map(|grab| grab.shortcut)
    }

    /// Computes all modifier mask permutations (accounting for CapsLock and NumLock)
    /// required for robust passive grabs in X11.
    pub fn compute_modifier_masks(&self, modifiers: ShortcutModifiers) -> Vec<ModMask> {
        let base_mask = x11_modifier_mask(modifiers);

        let mut masks = vec![base_mask, base_mask | ModMask::LOCK];

        if let Some(num_lock) = self.num_lock_mask {
            masks.push(base_mask | num_lock);

            masks.push(base_mask | ModMask::LOCK | num_lock);
        }

        masks
    }

    /// Executes passive key grabs for a given keycode and slice of modifier masks.
    ///
    /// If grabbing any mask fails or encounters a conflict, all masks acquired
    /// during this batch are immediately ungrabbed and rolled back, ensuring
    /// zero dangling grabs are left on the root window.
    fn execute_grab(
        &self,
        keycode: u8,
        masks: &[ModMask],
        shortcut: Shortcut,
    ) -> Result<Vec<ModMask>, ShortcutError> {
        let mut acquired = Vec::with_capacity(masks.len());

        for &mask in masks {
            let cookie = self
                .connection
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
                        "failed to send X11 grab request for '{shortcut}': {error}"
                    ))
                })?;

            match cookie.check() {
                Ok(()) => {
                    acquired.push(mask);
                }
                Err(error) => {
                    // Rollback only the masks acquired during this failed attempt
                    for &rollback_mask in &acquired {
                        if let Ok(cookie) =
                            self.connection
                                .ungrab_key(keycode, self.root_window, rollback_mask)
                        {
                            let _ = cookie.check();
                        }
                    }
                    let _ = self.connection.flush();

                    return Err(classify_x11_error(error, shortcut));
                }
            }
        }

        self.connection.flush().map_err(|error| {
            for &rollback_mask in &acquired {
                if let Ok(cookie) =
                    self.connection
                        .ungrab_key(keycode, self.root_window, rollback_mask)
                {
                    let _ = cookie.check();
                }
            }
            let _ = self.connection.flush();

            ShortcutError::Failed(format!("failed to flush X11 grab registration: {error}"))
        })?;

        Ok(acquired)
    }

    /// Releases any currently active root window grab and resets key state.
    fn release_grab(&mut self) -> Result<(), ShortcutError> {
        if let Some(grab) = self.registered_grab.take() {
            for mask in grab.masks {
                if let Ok(cookie) = self
                    .connection
                    .ungrab_key(grab.keycode, self.root_window, mask)
                {
                    let _ = cookie.check();
                }
            }
            let _ = self.connection.flush();
        }

        self.key_down = false;
        self.pending_event = None;

        Ok(())
    }

    fn next_event(&mut self) -> Result<x11rb::protocol::Event, ShortcutError> {
        if let Some(event) = self.pending_event.take() {
            return Ok(event);
        }

        use std::os::unix::io::AsRawFd;

        loop {
            // Drain already-buffered X11 events before blocking on raw file descriptors
            if let Some(event) = self.connection.poll_for_event().map_err(|error| {
                ShortcutError::Failed(format!("failed polling X11 event: {error}"))
            })? {
                return Ok(event);
            }

            let conn_fd = self.connection.stream().as_raw_fd();
            let wake_fd = self.wake_reader.as_raw_fd();

            let mut fds = [
                libc::pollfd {
                    fd: conn_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: wake_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];

            let ret = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if ret < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(ShortcutError::Failed(format!("libc::poll failed: {err}")));
            }

            if fds[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                self.drain_wake_pipe();
                return Err(ShortcutError::Interrupted);
            }

            if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Err(ShortcutError::Failed(
                    "X11 connection socket error or hangup".to_string(),
                ));
            }

            // Connection FD is readable; loop will poll_for_event() at start of next iteration
        }
    }
}

impl ShortcutBackend for X11ShortcutBackend {
    fn name(&self) -> &'static str {
        "X11 global shortcut"
    }

    fn capability(&self) -> ShortcutBackendCapability {
        ShortcutBackendCapability::Native
    }

    fn register(
        &mut self,
        shortcut: Shortcut,
    ) -> Result<ShortcutRegistrationOutcome, ShortcutError> {
        // Idempotent: re-registering the exact same shortcut is a no-op
        if self.registered_shortcut() == Some(shortcut) {
            return Ok(ShortcutRegistrationOutcome::Active {
                description: format!("X11 root window grab for {shortcut}"),
            });
        }

        let keysym = shortcut_keysym(shortcut.key)?;

        let keycode = find_keycode(&self.connection, keysym)?;

        let masks = self.compute_modifier_masks(shortcut.modifiers);

        // Transactional Re-Registration:
        // Attempt new grab FIRST. If this fails or conflicts, execute_grab rolls back
        // its own newly attempted masks, leaving self.registered_grab completely intact.
        let acquired_masks = self.execute_grab(keycode, &masks, shortcut)?;

        // Only after the replacement grab succeeds do we release the previous grab
        let _ = self.release_grab();

        self.registered_grab = Some(RegisteredGrab {
            shortcut,
            keycode,
            masks: acquired_masks,
        });

        self.key_down = false;

        self.pending_event = None;

        Ok(ShortcutRegistrationOutcome::Active {
            description: format!("X11 root window grab for {shortcut}"),
        })
    }

    fn wait_for_activation(&mut self) -> Result<ShortcutActivation, ShortcutError> {
        let Some(ref grab) = self.registered_grab else {
            return Err(ShortcutError::Failed(
                "shortcut backend has not been registered".to_string(),
            ));
        };

        let keycode = grab.keycode;

        loop {
            let event = self.next_event()?;

            match event {
                x11rb::protocol::Event::KeyPress(event) if event.detail == keycode => {
                    if self.key_down {
                        continue;
                    }

                    self.key_down = true;

                    return Ok(ShortcutActivation::none());
                }

                x11rb::protocol::Event::KeyRelease(release) if release.detail == keycode => {
                    let next_event = self.connection.poll_for_event().map_err(|error| {
                        ShortcutError::Failed(format!("failed checking X11 repeat event: {error}"))
                    })?;

                    if let Some(x11rb::protocol::Event::KeyPress(press)) = next_event {
                        if press.detail == release.detail && press.time == release.time {
                            /*
                             * Classic X11 auto-repeat:
                             *
                             * KeyRelease + KeyPress with
                             * identical keycode/time.
                             *
                             * The key is still physically
                             * held, so do not re-arm.
                             */
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

    fn unregister(&mut self) -> Result<(), ShortcutError> {
        self.release_grab()
    }

    fn wake_handle(&self) -> Option<std::os::unix::net::UnixStream> {
        self.wake_writer.try_clone().ok()
    }

    fn wake(&self) -> Result<(), ShortcutError> {
        use std::io::Write;
        (&self.wake_writer)
            .write_all(&[1])
            .map_err(|e| ShortcutError::Failed(format!("failed to write to X11 wake pipe: {e}")))?;
        Ok(())
    }

    fn wake_trigger(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        let writer = self.wake_writer.try_clone().ok()?;
        Some(std::sync::Arc::new(move || {
            use std::io::Write;
            let _ = (&writer).write_all(&[1]);
        }))
    }
}

impl Drop for X11ShortcutBackend {
    fn drop(&mut self) {
        let _ = self.release_grab();
    }
}

/// Classifies X11 grab reply errors into semantic `ShortcutError` variants.
fn classify_x11_error(error: ReplyError, shortcut: Shortcut) -> ShortcutError {
    match error {
        ReplyError::X11Error(ref x11_err) => {
            if x11_err.error_kind == ErrorKind::Access || x11_err.error_code == 10 {
                ShortcutError::Conflict(format!(
                    "shortcut '{shortcut}' conflicts with an existing X11 grab (BadAccess)"
                ))
            } else {
                ShortcutError::Failed(format!(
                    "X11 error registering shortcut '{shortcut}': {x11_err:?}"
                ))
            }
        }
        ReplyError::ConnectionError(ref err) => ShortcutError::Failed(format!(
            "X11 connection error registering shortcut '{shortcut}': {err}"
        )),
    }
}

pub fn shortcut_keysym(key: ShortcutKey) -> Result<u32, ShortcutError> {
    match key {
        ShortcutKey::Character(character) if character.is_ascii_alphanumeric() => {
            Ok(character.to_ascii_lowercase() as u32)
        }

        ShortcutKey::Character(_) => Err(ShortcutError::Unavailable),

        ShortcutKey::Named(named) => Ok(named.keysym()),
    }
}

pub fn x11_modifier_mask(modifiers: ShortcutModifiers) -> ModMask {
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
        "could not resolve keysym {keysym:#x} on active keyboard layout"
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
