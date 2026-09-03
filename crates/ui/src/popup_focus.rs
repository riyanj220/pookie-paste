use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt as _, EventMask, Window,
};

pub fn request_focus() -> bool {
    try_request_focus().unwrap_or(false)
}

fn try_request_focus() -> Result<bool, Box<dyn std::error::Error>> {
    if std::env::var("XDG_SESSION_TYPE")
        .map(|value| value != "x11")
        .unwrap_or(true)
    {
        return Ok(false);
    }

    let (connection, screen_num) = x11rb::connect(None)?;

    let root = connection.setup().roots[screen_num].root;

    let client_list_atom = intern_atom(&connection, b"_NET_CLIENT_LIST")?;

    let pid_atom = intern_atom(&connection, b"_NET_WM_PID")?;

    let active_window_atom = intern_atom(&connection, b"_NET_ACTIVE_WINDOW")?;

    let clients = connection
        .get_property(false, root, client_list_atom, AtomEnum::WINDOW, 0, u32::MAX)?
        .reply()?;

    let Some(windows) = clients.value32() else {
        return Ok(false);
    };

    let current_pid = std::process::id();

    for window in windows {
        if window_belongs_to_pid(&connection, window, pid_atom, current_pid)? {
            activate_window(&connection, root, window, active_window_atom)?;

            return Ok(true);
        }
    }

    Ok(false)
}

fn intern_atom(
    connection: &x11rb::rust_connection::RustConnection,
    name: &[u8],
) -> Result<u32, Box<dyn std::error::Error>> {
    Ok(connection.intern_atom(false, name)?.reply()?.atom)
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

fn activate_window(
    connection: &x11rb::rust_connection::RustConnection,
    root: Window,
    window: Window,
    active_window_atom: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let event = ClientMessageEvent::new(
        32,
        window,
        active_window_atom,
        ClientMessageData::from([1, 0, 0, 0, 0]),
    );
    connection.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )?;

    connection.flush()?;

    Ok(())
}
