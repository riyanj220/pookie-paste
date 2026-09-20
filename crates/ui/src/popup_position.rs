use crate::popup_anchor::{AnchorSource, PopupAnchor, ScreenDimensions};

pub fn popup_position(
    anchor: PopupAnchor,
    popup_width: f32,
    popup_height: f32,
    cursor_offset: f32,
    screen: ScreenDimensions,
) -> [f32; 2] {
    calculate_popup_position(
        anchor,
        popup_width,
        popup_height,
        cursor_offset,
        screen.width,
        screen.height,
    )
}

pub fn calculate_popup_position(
    anchor: PopupAnchor,
    popup_width: f32,
    popup_height: f32,
    cursor_offset: f32,
    screen_width: f32,
    screen_height: f32,
) -> [f32; 2] {
    let (mut x, mut y) = match anchor.source {
        AnchorSource::FocusTarget => {
            let x = anchor.x - (popup_width / 2.0);
            let y = anchor.y - (popup_height / 2.0);

            (x, y)
        }

        AnchorSource::Mouse => {
            /*
             * Prefer opening below and to the right of
             * the cursor.
             */
            let mut x = anchor.x + cursor_offset;
            let mut y = anchor.y + cursor_offset;

            /*
             * If the popup would extend beyond the right
             * edge, flip it to the left side of the cursor.
             */
            if x + popup_width > screen_width {
                x = anchor.x - cursor_offset - popup_width;
            }

            /*
             * If the popup would extend beyond the bottom
             * edge, flip it above the cursor.
             */
            if y + popup_height > screen_height {
                y = anchor.y - cursor_offset - popup_height;
            }

            (x, y)
        }
    };

    /*
     * Safety clamping handles:
     * - anchor extremely close to top/left edge
     * - popup larger than the available screen area
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

    fn mouse_anchor(x: f32, y: f32) -> PopupAnchor {
        PopupAnchor {
            x,
            y,
            source: AnchorSource::Mouse,
        }
    }

    fn focus_target_anchor(x: f32, y: f32) -> PopupAnchor {
        PopupAnchor {
            x,
            y,
            source: AnchorSource::FocusTarget,
        }
    }

    #[test]
    fn positions_popup_below_and_right_normally() {
        let position = calculate_popup_position(
            mouse_anchor(500.0, 300.0),
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [512.0, 312.0]);
    }

    #[test]
    fn flips_popup_left_near_right_edge() {
        let position = calculate_popup_position(
            mouse_anchor(1800.0, 300.0),
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [1428.0, 312.0]);
    }

    #[test]
    fn flips_popup_above_near_bottom_edge() {
        let position = calculate_popup_position(
            mouse_anchor(500.0, 1000.0),
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [512.0, 568.0]);
    }

    #[test]
    fn flips_popup_left_and_above_near_bottom_right() {
        let position = calculate_popup_position(
            mouse_anchor(1800.0, 1000.0),
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [1428.0, 568.0]);
    }

    #[test]
    fn clamps_popup_when_flipping_would_cross_left_edge() {
        let position = calculate_popup_position(
            mouse_anchor(100.0, 300.0),
            500.0,
            POPUP_HEIGHT,
            OFFSET,
            550.0,
            SCREEN_HEIGHT,
        );

        assert_eq!(position[0], 0.0);
    }

    #[test]
    fn clamps_popup_when_larger_than_screen() {
        let position = calculate_popup_position(
            mouse_anchor(500.0, 300.0),
            2000.0,
            1200.0,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [0.0, 0.0]);
    }

    #[test]
    fn centers_popup_on_focus_target_anchor() {
        let position = calculate_popup_position(
            focus_target_anchor(960.0, 540.0),
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(
            position,
            [960.0 - (POPUP_WIDTH / 2.0), 540.0 - (POPUP_HEIGHT / 2.0)]
        );
    }

    #[test]
    fn clamps_focus_target_popup_near_left_and_top_edges() {
        let position = calculate_popup_position(
            focus_target_anchor(50.0, 50.0),
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [0.0, 0.0]);
    }

    #[test]
    fn clamps_focus_target_popup_near_right_and_bottom_edges() {
        let position = calculate_popup_position(
            focus_target_anchor(1900.0, 1060.0),
            POPUP_WIDTH,
            POPUP_HEIGHT,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(
            position,
            [SCREEN_WIDTH - POPUP_WIDTH, SCREEN_HEIGHT - POPUP_HEIGHT]
        );
    }

    #[test]
    fn focus_target_clamps_when_larger_than_screen() {
        let position = calculate_popup_position(
            focus_target_anchor(500.0, 500.0),
            2000.0,
            1200.0,
            OFFSET,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
        );

        assert_eq!(position, [0.0, 0.0]);
    }
}
