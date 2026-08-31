use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::wrapper::ConnectionExt as _;

use crate::paste_backend::{PasteBackend, PasteCapability, PasteError};

const XK_CONTROL_L: u32 = 0xffe3;
const XK_V: u32 = 0x0076;

pub struct X11PasteBackend {
    connection: x11rb::rust_connection::RustConnection,
}

impl X11PasteBackend {
    pub fn new() -> Result<Self, PasteError> {
        let (connection, _) = x11rb::connect(None)
            .map_err(|error| PasteError::Failed(format!("failed to connect to X11: {error}")))?;

        Ok(Self { connection })
    }

    fn keycode_for_keysym(&self, keysym: u32) -> Result<u8, PasteError> {
        let setup = self.connection.setup();

        let min = setup.min_keycode;

        let max = setup.max_keycode;

        let count = max - min + 1;

        let reply = self
            .connection
            .get_keyboard_mapping(min, count)
            .map_err(|error| {
                PasteError::Failed(format!("failed to request keyboard mapping: {error}"))
            })?
            .reply()
            .map_err(|error| {
                PasteError::Failed(format!("failed to read keyboard mapping: {error}"))
            })?;

        let per_keycode = reply.keysyms_per_keycode as usize;

        for (index, keysyms) in reply.keysyms.chunks(per_keycode).enumerate() {
            if keysyms.contains(&keysym) {
                return Ok(min + index as u8);
            }
        }

        Err(PasteError::Failed(format!(
            "no keycode found for keysym {keysym:#x}"
        )))
    }

    fn send_key(&self, keycode: u8, pressed: bool) -> Result<(), PasteError> {
        let event_type = if pressed { 2 } else { 3 };

        self.connection
            .xtest_fake_input(event_type, keycode, 0, x11rb::NONE, 0, 0, 0)
            .map_err(|error| {
                PasteError::Failed(format!("failed to send X11 key event: {error}"))
            })?;

        Ok(())
    }
}

impl PasteBackend for X11PasteBackend {
    fn capability(&self) -> PasteCapability {
        PasteCapability::Direct
    }

    fn paste(&self) -> Result<(), PasteError> {
        let control = self.keycode_for_keysym(XK_CONTROL_L)?;

        let v = self.keycode_for_keysym(XK_V)?;

        self.send_key(control, true)?;

        self.send_key(v, true)?;

        self.send_key(v, false)?;

        self.send_key(control, false)?;

        self.connection.sync().map_err(|error| {
            PasteError::Failed(format!("failed to synchronize X11 paste events: {error}"))
        })?;

        Ok(())
    }
}
