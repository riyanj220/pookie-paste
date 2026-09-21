//! X11 window placement and native focus acquisition.
//!
//! Encapsulates X11 geometry resolution and native `_NET_ACTIVE_WINDOW` focus acquisition
//! while preserving the proven timing, retry intervals, and coordinate calculations.

use crate::popup_anchor::resolve_popup_anchor;
pub(crate) use crate::popup_focus::{FocusRequestState, request_focus};
use crate::popup_position::popup_position;

/// Resolves initial popup window coordinates under X11.
///
/// Discovers the active target window or cursor position using x11rb,
/// then calculates the clamped screen position.
pub fn resolve_popup_position(
    target_id: Option<&ipc::IpcFocusTarget>,
    popup_width: f32,
    popup_height: f32,
    cursor_offset: f32,
) -> Option<[f32; 2]> {
    let resolved = resolve_popup_anchor(target_id)?;

    Some(popup_position(
        resolved.anchor,
        popup_width,
        popup_height,
        cursor_offset,
        resolved.screen,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_placement_without_x11_session_returns_none() {
        // In unit test environment where XDG_SESSION_TYPE is not set to x11, returns None
        if std::env::var("XDG_SESSION_TYPE").unwrap_or_default() != "x11" {
            assert_eq!(resolve_popup_position(None, 360.0, 420.0, 12.0), None);
        }
    }
}
