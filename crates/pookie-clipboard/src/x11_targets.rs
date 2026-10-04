use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::errors::{ConnectError, ConnectionError, ReplyError, ReplyOrIdError};
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEvent, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

///
/// Maximum safety ceiling for reading an X11 `text/uri-list` property from the server.
///
/// Set to 64 KiB (65,536 bytes).
///
/// Rationale:
/// - Pookie's file-backed image ingestion contract supports exactly ONE local file URI.
/// - A typical URI string (e.g. `file:///home/user/photo.png\r\n`) is 30–250 bytes.
/// - 64 KiB allows exceptionally long paths while strictly bounding memory consumption
///   and instantly rejecting malformed or hostile X11 property payloads.
/// - Completely distinct from the 32 MiB local image source-file read ceiling and
///   the 32 MiB canonical PNG history ceiling.
///
pub const MAX_X11_URI_LIST_BYTES: u32 = 64 * 1024;

///
/// Errors that can occur during X11 selection and target resolution.
///
#[derive(Debug)]
pub enum X11SelectionError {
    Connect(ConnectError),
    Connection(ConnectionError),
    Reply(ReplyError),
    ReplyOrId(ReplyOrIdError),
    ConversionRefused,
    Timeout,
    UnsupportedIncr,
    InvalidFormat { expected: u8, actual: u8 },
    InvalidAtomData,
    OversizedPayload { size: usize, max: usize },
}

impl fmt::Display for X11SelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(error) => {
                write!(formatter, "X11 connect error: {error}")
            }

            Self::Connection(error) => {
                write!(formatter, "X11 connection error: {error}")
            }

            Self::Reply(error) => {
                write!(formatter, "X11 reply error: {error}")
            }

            Self::ReplyOrId(error) => {
                write!(formatter, "X11 ID allocation error: {error}")
            }

            Self::ConversionRefused => {
                write!(
                    formatter,
                    "X11 selection owner refused conversion (property NONE)"
                )
            }

            Self::Timeout => {
                write!(formatter, "timeout waiting for X11 SelectionNotify event")
            }

            Self::UnsupportedIncr => {
                write!(formatter, "selection target uses unsupported INCR transfer")
            }

            Self::InvalidFormat { expected, actual } => {
                write!(
                    formatter,
                    "invalid property format: expected {expected}-bit, got {actual}-bit"
                )
            }

            Self::InvalidAtomData => {
                write!(
                    formatter,
                    "property does not contain valid 32-bit atom data"
                )
            }

            Self::OversizedPayload { size, max } => {
                write!(
                    formatter,
                    "X11 property payload ({size} bytes) exceeds safety ceiling of {max} bytes"
                )
            }
        }
    }
}

impl std::error::Error for X11SelectionError {}

impl From<ConnectError> for X11SelectionError {
    fn from(error: ConnectError) -> Self {
        Self::Connect(error)
    }
}

impl From<ConnectionError> for X11SelectionError {
    fn from(error: ConnectionError) -> Self {
        Self::Connection(error)
    }
}

impl From<ReplyError> for X11SelectionError {
    fn from(error: ReplyError) -> Self {
        Self::Reply(error)
    }
}

impl From<ReplyOrIdError> for X11SelectionError {
    fn from(error: ReplyOrIdError) -> Self {
        Self::ReplyOrId(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCapabilities {
    #[allow(dead_code)]
    pub has_direct_image: bool,
    pub has_uri_list: bool,
    pub has_text: bool,
}

impl TargetCapabilities {
    pub fn from_atoms(atoms: &[Atom], known: &KnownAtoms) -> Self {
        let mut has_direct_image = false;
        let mut has_uri_list = false;
        let mut has_text = false;

        for &atom in atoms {
            if known.is_direct_image(atom) {
                has_direct_image = true;
            } else if atom == known.text_uri_list {
                has_uri_list = true;
            } else if known.is_text(atom) {
                has_text = true;
            }
        }

        Self {
            has_direct_image,
            has_uri_list,
            has_text,
        }
    }
}

///
/// Internal tracker for clipboard generations and cached target capabilities.
///
#[derive(Debug)]
pub(crate) struct GenerationTracker {
    generation: u64,
    cached_generation: u64,
    cached_capabilities: Option<TargetCapabilities>,
}

impl GenerationTracker {
    pub fn new() -> Self {
        Self {
            generation: 1,
            cached_generation: 0,
            cached_capabilities: None,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn notify_clipboard_change(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.cached_generation = 0;
        self.cached_capabilities = None;
    }

    pub fn get_cached(&self) -> Option<TargetCapabilities> {
        if self.generation == self.cached_generation {
            self.cached_capabilities
        } else {
            None
        }
    }

    pub fn update_cache(&mut self, capabilities: TargetCapabilities) {
        self.cached_generation = self.generation;
        self.cached_capabilities = Some(capabilities);
    }

    pub fn invalidate(&mut self) {
        self.cached_generation = 0;
        self.cached_capabilities = None;
    }
}

///
/// Pure validator for X11 `text/uri-list` property metadata and bytes.
///
pub(crate) fn validate_uri_list_property(
    prop_type: Atom,
    incr_atom: Atom,
    format: u8,
    bytes_len: usize,
    bytes_after: u32,
    max_bytes: u32,
) -> Result<(), X11SelectionError> {
    if prop_type == incr_atom {
        return Err(X11SelectionError::UnsupportedIncr);
    }

    if format != 8 {
        return Err(X11SelectionError::InvalidFormat {
            expected: 8,
            actual: format,
        });
    }

    if bytes_after > 0 || bytes_len as u32 > max_bytes {
        return Err(X11SelectionError::OversizedPayload {
            size: bytes_len + bytes_after as usize,
            max: max_bytes as usize,
        });
    }

    Ok(())
}

///
/// Pure validator for X11 `TARGETS` property metadata.
///
pub(crate) fn validate_targets_property(
    prop_type: Atom,
    atom_type: Atom,
    incr_atom: Atom,
    format: u8,
    bytes_after: u32,
) -> Result<(), X11SelectionError> {
    if prop_type == incr_atom {
        return Err(X11SelectionError::UnsupportedIncr);
    }

    if prop_type != atom_type {
        return Err(X11SelectionError::InvalidAtomData);
    }

    if format != 32 {
        return Err(X11SelectionError::InvalidFormat {
            expected: 32,
            actual: format,
        });
    }

    if bytes_after > 0 {
        return Err(X11SelectionError::OversizedPayload {
            size: 4096 + bytes_after as usize,
            max: 4096,
        });
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct KnownAtoms {
    pub clipboard: Atom,
    pub targets: Atom,
    pub text_uri_list: Atom,
    pub utf8_string: Atom,
    pub string: Atom,
    pub text: Atom,
    #[allow(dead_code)]
    pub image_png: Atom,
    #[allow(dead_code)]
    pub image_jpeg: Atom,
    #[allow(dead_code)]
    pub image_webp: Atom,
    #[allow(dead_code)]
    pub image_bmp: Atom,
    #[allow(dead_code)]
    pub image_gif: Atom,
    pub incr: Atom,
    pub pookie_selection: Atom,
    pub timestamp: Atom,
}

impl KnownAtoms {
    pub fn new(conn: &impl Connection) -> Result<Self, X11SelectionError> {
        let clipboard = conn.intern_atom(false, b"CLIPBOARD")?.reply()?.atom;
        let targets = conn.intern_atom(false, b"TARGETS")?.reply()?.atom;
        let text_uri_list = conn.intern_atom(false, b"text/uri-list")?.reply()?.atom;
        let utf8_string = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
        let string = AtomEnum::STRING.into();
        let text = conn.intern_atom(false, b"TEXT")?.reply()?.atom;
        let image_png = conn.intern_atom(false, b"image/png")?.reply()?.atom;
        let image_jpeg = conn.intern_atom(false, b"image/jpeg")?.reply()?.atom;
        let image_webp = conn.intern_atom(false, b"image/webp")?.reply()?.atom;
        let image_bmp = conn.intern_atom(false, b"image/bmp")?.reply()?.atom;
        let image_gif = conn.intern_atom(false, b"image/gif")?.reply()?.atom;
        let incr = conn.intern_atom(false, b"INCR")?.reply()?.atom;
        let pookie_selection = conn.intern_atom(false, b"_POOKIE_SELECTION")?.reply()?.atom;
        let timestamp = conn.intern_atom(false, b"TIMESTAMP")?.reply()?.atom;

        Ok(Self {
            clipboard,
            targets,
            text_uri_list,
            utf8_string,
            string,
            text,
            image_png,
            image_jpeg,
            image_webp,
            image_bmp,
            image_gif,
            incr,
            pookie_selection,
            timestamp,
        })
    }

    #[allow(dead_code)]
    pub fn is_direct_image(&self, atom: Atom) -> bool {
        atom == self.image_png
            || atom == self.image_jpeg
            || atom == self.image_webp
            || atom == self.image_bmp
            || atom == self.image_gif
    }

    pub fn is_text(&self, atom: Atom) -> bool {
        atom == self.utf8_string || atom == self.string || atom == self.text
    }
}

pub struct X11TargetReader {
    conn: RustConnection,
    requestor: Window,
    atoms: KnownAtoms,
    xfixes_active: bool,
    tracker: GenerationTracker,
}

impl X11TargetReader {
    pub fn new() -> Result<Self, X11SelectionError> {
        let (conn, screen_num) = x11rb::connect(None)?;

        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let requestor = conn.generate_id()?;

        let create_cookie = conn.create_window(
            0,
            requestor,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )?;
        create_cookie.check()?;

        let atoms = KnownAtoms::new(&conn)?;

        let xfixes_active = match conn.xfixes_query_version(5, 0) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => {
                    tracing::debug!(
                        major = reply.major_version,
                        minor = reply.minor_version,
                        "XFixes initialized on X11 clipboard connection"
                    );

                    let mask = SelectionEventMask::SET_SELECTION_OWNER
                        | SelectionEventMask::SELECTION_WINDOW_DESTROY
                        | SelectionEventMask::SELECTION_CLIENT_CLOSE;

                    match conn.xfixes_select_selection_input(requestor, atoms.clipboard, mask) {
                        Ok(sub_cookie) => match sub_cookie.check() {
                            Ok(_) => {
                                let _ = conn.flush();
                                true
                            }
                            Err(error) => {
                                tracing::warn!(
                                    error = %error,
                                    "XFixes selection input subscription rejected by server; falling back to per-read TARGETS query"
                                );
                                false
                            }
                        },
                        Err(error) => {
                            tracing::warn!(
                                error = %error,
                                "failed selecting XFixes selection input; falling back to per-read TARGETS query"
                            );
                            false
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "XFixes query version failed; falling back to per-read TARGETS query"
                    );
                    false
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "XFixes not supported; falling back to per-read TARGETS query"
                );
                false
            }
        };

        Ok(Self {
            conn,
            requestor,
            atoms,
            xfixes_active,
            tracker: GenerationTracker::new(),
        })
    }

    ///
    /// Query target capabilities for the current CLIPBOARD owner.
    pub fn xfixes_active(&self) -> bool {
        self.xfixes_active
    }

    pub fn connection_fd(&self) -> std::os::fd::RawFd {
        use std::os::unix::io::AsRawFd;
        self.conn.stream().as_raw_fd()
    }

    pub fn poll_for_event(&self) -> Result<Option<Event>, X11SelectionError> {
        Ok(self.conn.poll_for_event()?)
    }

    pub fn get_selection_owner(&self) -> Result<Window, X11SelectionError> {
        let reply = self
            .conn
            .get_selection_owner(self.atoms.clipboard)?
            .reply()?;
        Ok(reply.owner)
    }

    pub fn atoms(&self) -> &KnownAtoms {
        &self.atoms
    }

    pub fn invalidate(&mut self) {
        self.tracker.invalidate();
    }

    pub fn notify_clipboard_change(&mut self) {
        self.tracker.notify_clipboard_change();
    }

    ///
    /// Query target capabilities for the current CLIPBOARD owner.
    ///
    /// Returns:
    /// - `Ok(Some(capabilities))` if an owner is present.
    /// - `Ok(None)` if `get_selection_owner()` is `NONE` (empty clipboard).
    /// - `Err(error)` if X11 communication or conversion failed.
    ///
    pub fn get_target_capabilities(
        &mut self,
        pending_events: &mut VecDeque<Event>,
    ) -> Result<Option<TargetCapabilities>, X11SelectionError> {
        let owner = self
            .conn
            .get_selection_owner(self.atoms.clipboard)?
            .reply()?
            .owner;

        if owner == x11rb::NONE {
            self.tracker.invalidate();
            return Ok(None);
        }

        if self.xfixes_active {
            self.drain_xfixes_events_into(pending_events)?;

            if let Some(cached) = self.tracker.get_cached() {
                tracing::debug!(
                    generation = self.tracker.generation(),
                    has_direct_image = cached.has_direct_image,
                    has_uri_list = cached.has_uri_list,
                    has_text = cached.has_text,
                    "target capabilities cache hit"
                );
                return Ok(Some(cached));
            }
        }

        tracing::debug!(
            generation = self.tracker.generation(),
            "target capabilities cache miss; querying targets from owner"
        );
        let capabilities = self.query_targets_from_owner(pending_events)?;
        tracing::debug!(
            generation = self.tracker.generation(),
            has_direct_image = capabilities.has_direct_image,
            has_uri_list = capabilities.has_uri_list,
            has_text = capabilities.has_text,
            "target capabilities queried from owner"
        );

        if self.xfixes_active {
            self.tracker.update_cache(capabilities);
        }

        Ok(Some(capabilities))
    }

    ///
    /// Read the `text/uri-list` payload from the current CLIPBOARD owner.
    ///
    pub fn read_uri_list_payload(
        &mut self,
        pending_events: &mut VecDeque<Event>,
    ) -> Result<Vec<u8>, X11SelectionError> {
        let _ = self
            .conn
            .delete_property(self.requestor, self.atoms.pookie_selection);
        let _ = self.conn.flush();

        self.conn.convert_selection(
            self.requestor,
            self.atoms.clipboard,
            self.atoms.text_uri_list,
            self.atoms.pookie_selection,
            x11rb::CURRENT_TIME,
        )?;
        self.conn.flush()?;

        let prop = self
            .wait_for_selection_notify(
                self.atoms.text_uri_list,
                Duration::from_millis(100),
                pending_events,
            )?
            .ok_or(X11SelectionError::ConversionRefused)?;

        let reply = self
            .conn
            .get_property(
                true,
                self.requestor,
                prop,
                AtomEnum::ANY,
                0,
                (MAX_X11_URI_LIST_BYTES / 4) + 1,
            )?
            .reply()?;

        validate_uri_list_property(
            reply.type_,
            self.atoms.incr,
            reply.format,
            reply.value.len(),
            reply.bytes_after,
            MAX_X11_URI_LIST_BYTES,
        )?;

        Ok(reply.value)
    }

    ///
    /// Read the ICCCM `TIMESTAMP` target from the current CLIPBOARD owner.
    ///
    /// Returns:
    /// - `Ok(Some(timestamp))` if the owner successfully converts TIMESTAMP.
    /// - `Ok(None)` if the owner refuses conversion or payload is invalid.
    /// - `Err(error)` on X11 protocol communication failure.
    ///
    pub fn read_selection_timestamp(
        &mut self,
        pending_events: &mut VecDeque<Event>,
    ) -> Result<Option<u32>, X11SelectionError> {
        let _ = self
            .conn
            .delete_property(self.requestor, self.atoms.pookie_selection);
        let _ = self.conn.flush();

        self.conn.convert_selection(
            self.requestor,
            self.atoms.clipboard,
            self.atoms.timestamp,
            self.atoms.pookie_selection,
            x11rb::CURRENT_TIME,
        )?;
        self.conn.flush()?;

        let prop = match self.wait_for_selection_notify(
            self.atoms.timestamp,
            Duration::from_millis(100),
            pending_events,
        )? {
            Some(prop) => prop,
            None => return Ok(None),
        };

        let reply = self
            .conn
            .get_property(true, self.requestor, prop, AtomEnum::ANY, 0, 1)?
            .reply()?;

        if reply.format != 32 || reply.value_len < 1 {
            return Ok(None);
        }

        let timestamp = reply.value32().and_then(|mut iter| iter.next());
        Ok(timestamp)
    }

    fn query_targets_from_owner(
        &mut self,
        pending_events: &mut VecDeque<Event>,
    ) -> Result<TargetCapabilities, X11SelectionError> {
        let _ = self
            .conn
            .delete_property(self.requestor, self.atoms.pookie_selection);
        let _ = self.conn.flush();

        self.conn.convert_selection(
            self.requestor,
            self.atoms.clipboard,
            self.atoms.targets,
            self.atoms.pookie_selection,
            x11rb::CURRENT_TIME,
        )?;
        self.conn.flush()?;

        let prop = self
            .wait_for_selection_notify(
                self.atoms.targets,
                Duration::from_millis(100),
                pending_events,
            )?
            .ok_or(X11SelectionError::ConversionRefused)?;

        let reply = self
            .conn
            .get_property(true, self.requestor, prop, AtomEnum::ATOM, 0, 1024)?
            .reply()?;

        validate_targets_property(
            reply.type_,
            AtomEnum::ATOM.into(),
            self.atoms.incr,
            reply.format,
            reply.bytes_after,
        )?;

        let atoms: Vec<Atom> = reply
            .value32()
            .ok_or(X11SelectionError::InvalidAtomData)?
            .collect();

        Ok(TargetCapabilities::from_atoms(&atoms, &self.atoms))
    }

    fn drain_xfixes_events_into(
        &mut self,
        pending_events: &mut VecDeque<Event>,
    ) -> Result<(), X11SelectionError> {
        while let Some(event) = self.conn.poll_for_event()? {
            if let Event::XfixesSelectionNotify(notify) = event
                && notify.selection == self.atoms.clipboard
            {
                tracing::debug!(
                    owner = notify.owner,
                    subtype = ?notify.subtype,
                    "XFixesSelectionNotify received during drain; invalidating cache and queuing"
                );
                self.tracker.invalidate();
                if notify.subtype == SelectionEvent::SET_SELECTION_OWNER {
                    pending_events.push_back(Event::XfixesSelectionNotify(notify));
                }
            }
        }

        Ok(())
    }

    fn wait_for_selection_notify(
        &mut self,
        expected_target: Atom,
        timeout: Duration,
        pending_events: &mut VecDeque<Event>,
    ) -> Result<Option<Atom>, X11SelectionError> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(5);

        while start.elapsed() < timeout {
            while let Some(event) = self.conn.poll_for_event()? {
                match event {
                    Event::SelectionNotify(notify) => {
                        if notify.requestor == self.requestor
                            && notify.selection == self.atoms.clipboard
                            && notify.target == expected_target
                        {
                            if notify.property == x11rb::NONE {
                                return Ok(None);
                            } else if notify.property == self.atoms.pookie_selection {
                                return Ok(Some(notify.property));
                            } else {
                                tracing::debug!(
                                    property = notify.property,
                                    expected = self.atoms.pookie_selection,
                                    "ignoring SelectionNotify with unexpected property"
                                );
                            }
                        } else {
                            tracing::debug!(
                                requestor = notify.requestor,
                                target = notify.target,
                                expected = expected_target,
                                "ignoring non-matching SelectionNotify"
                            );
                        }
                    }

                    Event::XfixesSelectionNotify(notify)
                        if notify.selection == self.atoms.clipboard =>
                    {
                        tracing::debug!(
                            owner = notify.owner,
                            subtype = ?notify.subtype,
                            "XFixesSelectionNotify received during conversion wait; invalidating cache and queuing"
                        );
                        self.tracker.invalidate();
                        if notify.subtype == SelectionEvent::SET_SELECTION_OWNER {
                            pending_events.push_back(Event::XfixesSelectionNotify(notify));
                        }
                    }

                    _ => {}
                }
            }

            std::thread::sleep(poll_interval);
        }

        Err(X11SelectionError::Timeout)
    }
}

impl Drop for X11TargetReader {
    fn drop(&mut self) {
        let _ = self.conn.destroy_window(self.requestor);
        let _ = self.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GenerationTracker, KnownAtoms, MAX_X11_URI_LIST_BYTES, TargetCapabilities,
        X11SelectionError, validate_targets_property, validate_uri_list_property,
    };
    use x11rb::protocol::xproto::Atom;

    fn mock_atoms() -> KnownAtoms {
        KnownAtoms {
            clipboard: 1,
            targets: 2,
            text_uri_list: 3,
            utf8_string: 4,
            string: 5,
            text: 6,
            image_png: 7,
            image_jpeg: 8,
            image_webp: 9,
            image_bmp: 10,
            image_gif: 11,
            incr: 12,
            pookie_selection: 13,
            timestamp: 14,
        }
    }

    #[test]
    fn capability_priority_direct_image_wins() {
        let known = mock_atoms();
        let targets = vec![known.image_png, known.text_uri_list, known.utf8_string];
        let caps = TargetCapabilities::from_atoms(&targets, &known);

        assert!(caps.has_direct_image);
        assert!(caps.has_uri_list);
        assert!(caps.has_text);
    }

    #[test]
    fn capability_priority_uri_list_with_text_fallback() {
        let known = mock_atoms();
        let targets = vec![known.text_uri_list, known.utf8_string];
        let caps = TargetCapabilities::from_atoms(&targets, &known);

        assert!(!caps.has_direct_image);
        assert!(caps.has_uri_list);
        assert!(caps.has_text);
    }

    #[test]
    fn capability_priority_uri_list_without_text_fallback() {
        let known = mock_atoms();
        let targets = vec![known.text_uri_list];
        let caps = TargetCapabilities::from_atoms(&targets, &known);

        assert!(!caps.has_direct_image);
        assert!(caps.has_uri_list);
        assert!(!caps.has_text);
    }

    #[test]
    fn capability_priority_text_only() {
        let known = mock_atoms();
        let targets = vec![known.utf8_string];
        let caps = TargetCapabilities::from_atoms(&targets, &known);

        assert!(!caps.has_direct_image);
        assert!(!caps.has_uri_list);
        assert!(caps.has_text);
    }

    #[test]
    fn capability_unsupported_targets() {
        let known = mock_atoms();
        let targets: Vec<Atom> = vec![99, 100];
        let caps = TargetCapabilities::from_atoms(&targets, &known);

        assert!(!caps.has_direct_image);
        assert!(!caps.has_uri_list);
        assert!(!caps.has_text);
    }

    #[test]
    fn generation_tracker_cache_and_invalidation() {
        let mut tracker = GenerationTracker::new();
        assert_eq!(tracker.generation(), 1);
        assert_eq!(tracker.get_cached(), None);

        let caps = TargetCapabilities {
            has_direct_image: false,
            has_uri_list: true,
            has_text: true,
        };

        tracker.update_cache(caps);
        assert_eq!(tracker.get_cached(), Some(caps));

        // Clipboard change notification increments generation, invalidating cached capabilities
        tracker.notify_clipboard_change();
        assert_eq!(tracker.generation(), 2);
        assert_eq!(tracker.get_cached(), None);

        // Updating cache restores it for the new generation
        let new_caps = TargetCapabilities {
            has_direct_image: false,
            has_uri_list: false,
            has_text: true,
        };
        tracker.update_cache(new_caps);
        assert_eq!(tracker.get_cached(), Some(new_caps));

        // Invalidate explicitly (e.g. on owner NONE)
        tracker.invalidate();
        assert_eq!(tracker.get_cached(), None);
    }

    #[test]
    fn validate_uri_list_property_accepts_valid_payload() {
        let known = mock_atoms();
        let result = validate_uri_list_property(
            known.text_uri_list,
            known.incr,
            8,
            128,
            0,
            MAX_X11_URI_LIST_BYTES,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_uri_list_property_rejects_incr() {
        let known = mock_atoms();
        let result =
            validate_uri_list_property(known.incr, known.incr, 8, 0, 0, MAX_X11_URI_LIST_BYTES);
        assert!(matches!(result, Err(X11SelectionError::UnsupportedIncr)));
    }

    #[test]
    fn validate_uri_list_property_rejects_invalid_format() {
        let known = mock_atoms();
        let result = validate_uri_list_property(
            known.text_uri_list,
            known.incr,
            16,
            128,
            0,
            MAX_X11_URI_LIST_BYTES,
        );
        assert!(matches!(
            result,
            Err(X11SelectionError::InvalidFormat {
                expected: 8,
                actual: 16
            })
        ));
    }

    #[test]
    fn validate_uri_list_property_rejects_oversized_payload() {
        let known = mock_atoms();
        let result = validate_uri_list_property(
            known.text_uri_list,
            known.incr,
            8,
            (MAX_X11_URI_LIST_BYTES + 1) as usize,
            0,
            MAX_X11_URI_LIST_BYTES,
        );
        assert!(matches!(
            result,
            Err(X11SelectionError::OversizedPayload { .. })
        ));

        let result_after = validate_uri_list_property(
            known.text_uri_list,
            known.incr,
            8,
            100,
            50,
            MAX_X11_URI_LIST_BYTES,
        );
        assert!(matches!(
            result_after,
            Err(X11SelectionError::OversizedPayload { .. })
        ));
    }

    #[test]
    fn validate_targets_property_accepts_valid_payload() {
        let known = mock_atoms();
        let atom_type = 4; // AtomEnum::ATOM
        let result = validate_targets_property(atom_type, atom_type, known.incr, 32, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_targets_property_rejects_incr() {
        let known = mock_atoms();
        let atom_type = 4;
        let result = validate_targets_property(known.incr, atom_type, known.incr, 32, 0);
        assert!(matches!(result, Err(X11SelectionError::UnsupportedIncr)));
    }

    #[test]
    fn validate_targets_property_rejects_wrong_type() {
        let known = mock_atoms();
        let atom_type = 4;
        let wrong_type = 31; // AtomEnum::STRING
        let result = validate_targets_property(wrong_type, atom_type, known.incr, 32, 0);
        assert!(matches!(result, Err(X11SelectionError::InvalidAtomData)));
    }

    #[test]
    fn validate_targets_property_rejects_wrong_format() {
        let known = mock_atoms();
        let atom_type = 4;
        let result = validate_targets_property(atom_type, atom_type, known.incr, 8, 0);
        assert!(matches!(
            result,
            Err(X11SelectionError::InvalidFormat {
                expected: 32,
                actual: 8
            })
        ));
    }

    #[test]
    fn validate_targets_property_rejects_truncated_data() {
        let known = mock_atoms();
        let atom_type = 4;
        let result = validate_targets_property(atom_type, atom_type, known.incr, 32, 20);
        assert!(matches!(
            result,
            Err(X11SelectionError::OversizedPayload { .. })
        ));
    }

    #[test]
    fn known_atoms_interns_timestamp_target() {
        let known = mock_atoms();
        assert_eq!(known.timestamp, 14);
    }

    #[test]
    fn generation_tracker_differentiates_set_owner_and_invalidation() {
        let mut tracker = GenerationTracker::new();
        assert_eq!(tracker.generation(), 1);

        // SetSelectionOwner equivalent: notify change bumps generation
        tracker.notify_clipboard_change();
        assert_eq!(tracker.generation(), 2);

        // SelectionWindowDestroy / SelectionClientClose equivalent: invalidate clears cache without bumping generation
        let caps = TargetCapabilities {
            has_direct_image: false,
            has_uri_list: true,
            has_text: true,
        };
        tracker.update_cache(caps);
        assert_eq!(tracker.get_cached(), Some(caps));

        tracker.invalidate();
        assert_eq!(tracker.get_cached(), None);
        assert_eq!(tracker.generation(), 2);
    }

    #[test]
    fn capability_cache_invalidates_on_notify_clipboard_change() {
        let mut tracker = GenerationTracker::new();

        // 1. First selection: text-only capabilities cached
        let text_only = TargetCapabilities {
            has_direct_image: false,
            has_uri_list: false,
            has_text: true,
        };
        tracker.update_cache(text_only);
        assert_eq!(tracker.get_cached(), Some(text_only));

        // 2. Next SetSelectionOwner event arrives: tracker notified
        tracker.notify_clipboard_change();

        // 3. Capability cache must NOT return stale text-only result
        assert_eq!(
            tracker.get_cached(),
            None,
            "capability cache must miss after SetSelectionOwner"
        );

        // 4. Fresh URI-list capabilities cached for new selection
        let uri_list_and_text = TargetCapabilities {
            has_direct_image: false,
            has_uri_list: true,
            has_text: true,
        };
        tracker.update_cache(uri_list_and_text);
        assert_eq!(tracker.get_cached(), Some(uri_list_and_text));
        assert!(tracker.get_cached().unwrap().has_uri_list);
    }
}
