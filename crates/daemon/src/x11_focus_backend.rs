use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt as _, EventMask,
};

use crate::focus_backend::{FocusBackend, FocusError, FocusTarget};

pub struct X11FocusBackend {
    connection: x11rb::rust_connection::RustConnection,

    root_window: u32,

    net_active_window_atom: u32,
}

impl X11FocusBackend {
    pub fn new() -> Result<Self, FocusError> {
        let (connection, screen_num) = x11rb::connect(None)
            .map_err(|error| FocusError::Failed(format!("failed to connect to X11: {error}")))?;

        let root_window = connection
            .setup()
            .roots
            .get(screen_num)
            .ok_or_else(|| FocusError::Failed(format!("X11 screen {screen_num} does not exist")))?
            .root;

        let atom_reply = connection
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .map_err(|error| {
                FocusError::Failed(format!(
                    "failed to request _NET_ACTIVE_WINDOW atom: {error}"
                ))
            })?
            .reply()
            .map_err(|error| {
                FocusError::Failed(format!(
                    "failed to resolve _NET_ACTIVE_WINDOW atom: {error}"
                ))
            })?;

        Ok(Self {
            connection,
            root_window,
            net_active_window_atom: atom_reply.atom,
        })
    }

    fn target_window(target: FocusTarget) -> Result<u32, FocusError> {
        let id = target.x11_id().ok_or_else(|| {
            FocusError::Failed(format!(
                "non-X11 focus target passed to X11 focus backend: {target}",
            ))
        })?;

        u32::try_from(id).map_err(|_| FocusError::Failed(format!("invalid X11 window ID: {id}")))
    }
}

impl FocusBackend for X11FocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        let reply = self
            .connection
            .get_property(
                false,
                self.root_window,
                self.net_active_window_atom,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .map_err(|error| {
                FocusError::Failed(format!("failed to request active X11 window: {error}"))
            })?
            .reply()
            .map_err(|error| {
                FocusError::Failed(format!("failed to read active X11 window: {error}"))
            })?;

        let mut values = reply.value32().ok_or(FocusError::Unavailable)?;

        let window = values.next().ok_or(FocusError::Unavailable)?;

        if window == 0 {
            return Err(FocusError::Unavailable);
        }

        Ok(FocusTarget::x11(u64::from(window)))
    }

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError> {
        let window = Self::target_window(target)?;

        let event = ClientMessageEvent::new(
            32,
            window,
            self.net_active_window_atom,
            ClientMessageData::from([1, x11rb::CURRENT_TIME, 0, 0, 0]),
        );

        self.connection
            .send_event(
                false,
                self.root_window,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|error| {
                FocusError::Failed(format!("failed to request X11 focus restoration: {error}"))
            })?;

        self.connection.flush().map_err(|error| {
            FocusError::Failed(format!("failed to flush X11 focus request: {error}"))
        })?;

        Ok(())
    }

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError> {
        /*
         * Reject a target belonging to another platform rather
         * than accidentally comparing unrelated identifiers.
         */
        if target.x11_id().is_none() {
            return Err(FocusError::Failed(format!(
                "non-X11 focus target passed to X11 focus backend: {target}",
            )));
        }

        match self.active_target() {
            Ok(active) => Ok(active == target),

            Err(FocusError::Unavailable) => Ok(false),

            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_kde_target_as_x11_window() {
        let kde_id =
            uuid::Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("valid UUID");

        let result = X11FocusBackend::target_window(FocusTarget::kde(kde_id));

        match result {
            Err(FocusError::Failed(message)) => {
                assert!(message.contains("non-X11 focus target"));
            }

            _ => {
                panic!("expected platform mismatch failure");
            }
        }
    }

    #[test]
    fn converts_valid_x11_target_to_window_id() {
        let window =
            X11FocusBackend::target_window(FocusTarget::x11(12345)).expect("valid X11 target");

        assert_eq!(window, 12345);
    }
}
