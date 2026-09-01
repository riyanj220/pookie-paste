use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;

pub fn cursor_position() -> Option<[f32; 2]> {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok()?.to_lowercase();

    if session_type != "x11" {
        return None;
    }

    let (connection, screen_num) = x11rb::connect(None).ok()?;

    let root = connection.setup().roots.get(screen_num)?.root;

    let pointer = connection.query_pointer(root).ok()?.reply().ok()?;

    Some([f32::from(pointer.root_x), f32::from(pointer.root_y)])
}
