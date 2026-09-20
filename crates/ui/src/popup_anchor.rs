use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSource {
    FocusTarget,
    Mouse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopupAnchor {
    pub x: f32,
    pub y: f32,
    pub source: AnchorSource,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenDimensions {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedAnchorContext {
    pub anchor: PopupAnchor,
    pub screen: ScreenDimensions,
}

pub fn resolve_popup_anchor(
    target_id: Option<&ipc::IpcFocusTarget>,
) -> Option<ResolvedAnchorContext> {
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .ok()?
        .trim()
        .to_ascii_lowercase();

    if session_type != "x11" {
        return None;
    }

    let (connection, screen_num) = x11rb::connect(None).ok()?;
    let screen = connection.setup().roots.get(screen_num)?;
    let root = screen.root;

    let screen_dimensions = ScreenDimensions {
        width: f32::from(screen.width_in_pixels),
        height: f32::from(screen.height_in_pixels),
    };

    // Priority 1: FocusTarget
    if let Some(anchor) = resolve_focus_target_anchor(&connection, root, target_id) {
        return Some(ResolvedAnchorContext {
            anchor,
            screen: screen_dimensions,
        });
    }

    // Priority 2 Fallback: Mouse
    if let Some(anchor) = resolve_mouse_anchor(&connection, root) {
        return Some(ResolvedAnchorContext {
            anchor,
            screen: screen_dimensions,
        });
    }

    None
}

fn resolve_focus_target_anchor<C: Connection>(
    connection: &C,
    root: u32,
    target_id: Option<&ipc::IpcFocusTarget>,
) -> Option<PopupAnchor> {
    let window_id = match target_id {
        Some(ipc::IpcFocusTarget::X11(id)) => u32::try_from(*id).ok()?,
        _ => return None,
    };

    if window_id == 0 || window_id == root {
        return None;
    }

    let geom = connection.get_geometry(window_id).ok()?.reply().ok()?;
    let width = f32::from(geom.width);
    let height = f32::from(geom.height);

    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let coords = connection
        .translate_coordinates(window_id, root, 0, 0)
        .ok()?
        .reply()
        .ok()?;

    let root_x = f32::from(coords.dst_x);
    let root_y = f32::from(coords.dst_y);

    Some(PopupAnchor {
        x: root_x + (width / 2.0),
        y: root_y + (height / 2.0),
        source: AnchorSource::FocusTarget,
    })
}

fn resolve_mouse_anchor<C: Connection>(connection: &C, root: u32) -> Option<PopupAnchor> {
    let pointer = connection.query_pointer(root).ok()?.reply().ok()?;

    Some(PopupAnchor {
        x: f32::from(pointer.root_x),
        y: f32::from(pointer.root_y),
        source: AnchorSource::Mouse,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_types_can_be_instantiated() {
        let anchor = PopupAnchor {
            x: 100.0,
            y: 200.0,
            source: AnchorSource::FocusTarget,
        };

        assert_eq!(anchor.source, AnchorSource::FocusTarget);
        assert_eq!(anchor.x, 100.0);
        assert_eq!(anchor.y, 200.0);

        let mouse_anchor = PopupAnchor {
            x: 50.0,
            y: 75.0,
            source: AnchorSource::Mouse,
        };

        assert_eq!(mouse_anchor.source, AnchorSource::Mouse);
    }

    #[test]
    fn screen_dimensions_stores_width_and_height() {
        let screen = ScreenDimensions {
            width: 1920.0,
            height: 1080.0,
        };

        assert_eq!(screen.width, 1920.0);
        assert_eq!(screen.height, 1080.0);
    }
}
