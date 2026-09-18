use std::cell::RefCell;
use std::time::{Duration, Instant};

use tracing::debug;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt as _, EventMask, Window,
};

const FOCUS_ACQUISITION_TIMEOUT: Duration = Duration::from_millis(500);

/*
 * The UI itself repaints roughly every frame while focus is
 * pending, but there is no need to send _NET_ACTIVE_WINDOW
 * on every repaint.
 *
 * The first request is immediate. Later requests are lightly
 * throttled while the WM is processing activation.
 */
const FOCUS_REQUEST_RETRY_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRequestState {
    /*
     * The UI process exists, but the window manager has not
     * published the popup in _NET_CLIENT_LIST yet.
     *
     * Critically, the 500 ms activation timeout has NOT
     * started at this point.
     */
    WaitingForWindow,

    /*
     * The native popup exists and activation is currently
     * being requested/verified.
     */
    Activating,

    /*
     * _NET_ACTIVE_WINDOW confirms that Pookie currently
     * owns native X11 focus.
     */
    Acquired,

    /*
     * Native X11 focus handling does not apply to this
     * session or could not be initialized.
     */
    Unavailable,

    /*
     * A valid popup window existed, but the WM did not
     * activate it within the bounded acquisition period.
     */
    TimedOut,
}

thread_local! {
    /*
     * Pookie's popup has one UI thread and one native
     * top-level window.
     *
     * Keep a single X11 connection and cache the discovered
     * window ID instead of reconnecting, interning atoms,
     * and scanning the complete client list every frame.
     */
    static POPUP_FOCUS_STATE: RefCell<PopupFocusState> =
        const {
            RefCell::new(
                PopupFocusState::Uninitialized,
            )
        };
}

enum PopupFocusState {
    Uninitialized,

    Ready(Box<X11PopupFocus>),

    Unavailable,
}

struct X11PopupFocus {
    connection: x11rb::rust_connection::RustConnection,

    root: Window,

    client_list_atom: u32,

    pid_atom: u32,

    active_window_atom: u32,

    popup_window: Option<Window>,

    /*
     * This clock starts only after the popup has actually
     * appeared in the WM client list.
     *
     * UI startup/rendering time therefore cannot consume
     * the focus-acquisition budget.
     */
    activation_started_at: Option<Instant>,

    last_focus_request_at: Option<Instant>,
}

impl X11PopupFocus {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (connection, screen_num) = x11rb::connect(None)?;

        let root = connection.setup().roots[screen_num].root;

        let client_list_atom = intern_atom(&connection, b"_NET_CLIENT_LIST")?;

        let pid_atom = intern_atom(&connection, b"_NET_WM_PID")?;

        let active_window_atom = intern_atom(&connection, b"_NET_ACTIVE_WINDOW")?;

        Ok(Self {
            connection,

            root,

            client_list_atom,

            pid_atom,

            active_window_atom,

            popup_window: None,

            activation_started_at: None,

            last_focus_request_at: None,
        })
    }

    fn poll(&mut self) -> Result<FocusRequestState, Box<dyn std::error::Error>> {
        let popup_window = match self.popup_window {
            Some(window) => window,

            None => {
                let Some(window) = find_window_for_pid(
                    &self.connection,
                    self.root,
                    self.client_list_atom,
                    self.pid_atom,
                    std::process::id(),
                )?
                else {
                    /*
                     * This is a normal startup state.
                     *
                     * eframe may already be rendering while
                     * the WM has not yet published the native
                     * window in _NET_CLIENT_LIST.
                     *
                     * Do not start the activation timeout here.
                     */
                    return Ok(FocusRequestState::WaitingForWindow);
                };

                self.popup_window = Some(window);

                self.activation_started_at = Some(Instant::now());

                debug!(
                    window,
                    "X11 popup window discovered; starting focus acquisition"
                );

                window
            }
        };

        if current_active_window(&self.connection, self.root, self.active_window_atom)?
            == Some(popup_window)
        {
            debug!(window = popup_window, "X11 popup focus acquired");

            return Ok(FocusRequestState::Acquired);
        }

        let Some(started_at) = self.activation_started_at else {
            /*
             * popup_window and activation_started_at are
             * established together, so reaching this state
             * would indicate an internal invariant violation.
             *
             * Treat it conservatively as unavailable rather
             * than attempting an unbounded focus loop.
             */
            return Ok(FocusRequestState::Unavailable);
        };

        if started_at.elapsed() > FOCUS_ACQUISITION_TIMEOUT {
            debug!(
                window = popup_window,
                "X11 popup focus acquisition timed out"
            );

            return Ok(FocusRequestState::TimedOut);
        }

        if self.should_request_focus() {
            activate_window(
                &self.connection,
                self.root,
                popup_window,
                self.active_window_atom,
            )?;

            self.last_focus_request_at = Some(Instant::now());
        }

        Ok(FocusRequestState::Activating)
    }

    fn should_request_focus(&self) -> bool {
        self.last_focus_request_at
            .map(|last_request| last_request.elapsed() >= FOCUS_REQUEST_RETRY_INTERVAL)
            .unwrap_or(true)
    }
}

pub fn request_focus() -> FocusRequestState {
    if !is_x11_session() {
        return FocusRequestState::Unavailable;
    }

    POPUP_FOCUS_STATE.with(|state| {
        let mut state = state.borrow_mut();

        if matches!(*state, PopupFocusState::Uninitialized) {
            *state = match X11PopupFocus::new() {
                Ok(focus) => PopupFocusState::Ready(Box::new(focus)),

                Err(error) => {
                    debug!(
                        %error,
                        "X11 popup focus initialization failed"
                    );

                    PopupFocusState::Unavailable
                }
            };
        }

        match &mut *state {
            PopupFocusState::Ready(focus) => match focus.poll() {
                Ok(result) => result,

                Err(error) => {
                    debug!(
                        %error,
                        "X11 popup focus request failed"
                    );

                    FocusRequestState::Unavailable
                }
            },

            PopupFocusState::Uninitialized | PopupFocusState::Unavailable => {
                FocusRequestState::Unavailable
            }
        }
    })
}

fn is_x11_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.trim().eq_ignore_ascii_case("x11"))
        .unwrap_or(false)
}

fn intern_atom(
    connection: &x11rb::rust_connection::RustConnection,
    name: &[u8],
) -> Result<u32, Box<dyn std::error::Error>> {
    Ok(connection.intern_atom(false, name)?.reply()?.atom)
}

fn find_window_for_pid(
    connection: &x11rb::rust_connection::RustConnection,
    root: Window,
    client_list_atom: u32,
    pid_atom: u32,
    expected_pid: u32,
) -> Result<Option<Window>, Box<dyn std::error::Error>> {
    let clients = connection
        .get_property(false, root, client_list_atom, AtomEnum::WINDOW, 0, u32::MAX)?
        .reply()?;

    let Some(windows) = clients.value32() else {
        return Ok(None);
    };

    for window in windows {
        if window_belongs_to_pid(connection, window, pid_atom, expected_pid)? {
            return Ok(Some(window));
        }
    }

    Ok(None)
}

fn window_belongs_to_pid(
    connection: &x11rb::rust_connection::RustConnection,
    window: Window,
    pid_atom: u32,
    expected_pid: u32,
) -> Result<bool, Box<dyn std::error::Error>> {
    let reply = connection
        .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)?
        .reply()?;

    Ok(reply.value32().and_then(|mut values| values.next()) == Some(expected_pid))
}

fn current_active_window(
    connection: &x11rb::rust_connection::RustConnection,
    root: Window,
    active_window_atom: u32,
) -> Result<Option<Window>, Box<dyn std::error::Error>> {
    let reply = connection
        .get_property(false, root, active_window_atom, AtomEnum::WINDOW, 0, 1)?
        .reply()?;

    Ok(reply
        .value32()
        .and_then(|mut values| values.next())
        .filter(|window| *window != 0))
}

fn activate_window(
    connection: &x11rb::rust_connection::RustConnection,
    root: Window,
    window: Window,
    active_window_atom: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    /*
     * Preserve the activation request that was already proven
     * to work once the popup exists.
     *
     * The bug was the lifetime of the acquisition timer, not
     * the _NET_ACTIVE_WINDOW payload itself.
     */
    let event = ClientMessageEvent::new(
        32,
        window,
        active_window_atom,
        ClientMessageData::from([1, 0, 0, 0, 0]),
    );

    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )?
        .check()?;

    connection.flush()?;

    Ok(())
}
