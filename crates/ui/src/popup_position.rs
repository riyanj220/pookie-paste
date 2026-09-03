use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;

pub fn popup_position(popup_width: f32, popup_height: f32, cursor_offset: f32) -> Option<[f32; 2]> {
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

    let pointer = connection.query_pointer(root).ok()?.reply().ok()?;

    let cursor_x = f32::from(pointer.root_x);

    let cursor_y = f32::from(pointer.root_y);

    let screen_width = f32::from(screen.width_in_pixels);

    let screen_height = f32::from(screen.height_in_pixels);

    Some(calculate_popup_position(
        cursor_x,
        cursor_y,
        popup_width,
        popup_height,
        cursor_offset,
        screen_width,
        screen_height,
    ))
}

fn calculate_popup_position(
    cursor_x: f32,
    cursor_y: f32,
    popup_width: f32,
    popup_height: f32,
    cursor_offset: f32,
    screen_width: f32,
    screen_height: f32,
) -> [f32; 2] {
    /*
     * Prefer opening below and to the right of
     * the cursor.
     */
    let mut x = cursor_x + cursor_offset;

    let mut y = cursor_y + cursor_offset;

    /*
     * If the popup would extend beyond the right
     * edge, flip it to the left side of the cursor.
     */
    if x + popup_width > screen_width {
        x = cursor_x - cursor_offset - popup_width;
    }

    /*
     * If the popup would extend beyond the bottom
     * edge, flip it above the cursor.
     */
    if y + popup_height > screen_height {
        y = cursor_y - cursor_offset - popup_height;
    }

    /*
     * Flipping is preferred, but still clamp as a
     * final safety measure.
     *
     * This handles:
     *
     * - cursor extremely close to the top/left edge
     * - popup larger than the available root area
     * - unusual X11 root dimensions
     */
    let max_x = (screen_width - popup_width).max(0.0);

    let max_y = (screen_height - popup_height).max(0.0);

    x = x.clamp(0.0, max_x);

    y = y.clamp(0.0, max_y);

    [x, y]
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPUP_WIDTH: f32 = 360.0;
    const POPUP_HEIGHT: f32 = 420.0;
    const OFFSET: f32 = 12.0;

    const SCREEN_WIDTH: f32 = 1920.0;
    const SCREEN_HEIGHT: f32 = 1080.0;

    #[test]
    fn positions_popup_below_and_right_normally() {
        let position = calculate_popup_position(
            500.0,
            300.0,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [512.0, 312.0],);
    }

    #[test]
    fn flips_popup_left_near_right_edge() {
        let position = calculate_popup_position(
            1800.0,
            300.0,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [1428.0, 312.0],);
    }

    #[test]
    fn flips_popup_above_near_bottom_edge() {
        let position = calculate_popup_position(
            500.0,
            1000.0,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [512.0, 568.0],);
    }

    #[test]
    fn flips_popup_left_and_above_near_bottom_right() {
        let position = calculate_popup_position(
            1800.0,
            1000.0,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [1428.0, 568.0],);
    }

    #[test]
    fn clamps_popup_when_flipping_would_cross_left_edge() {
        let position = calculate_popup_position(
            100.0,
            300.0,
            500.0,
            POPUP_HEIGHT,
            OFFSET,
            550.0,
            SCREEN_HEIGHT,
        );

        assert_eq!(position[0], 0.0,);
    }

    #[test]
    fn clamps_popup_when_larger_than_screen() {
        let position = calculate_popup_position(
            500.0,
            300.0,
            2000.0,
            1200.0,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [0.0, 0.0],);
    }
}
