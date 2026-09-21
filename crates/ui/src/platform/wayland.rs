//! Wayland window placement and focus acquisition.
//!
//! Under standard Wayland security, clients cannot query arbitrary foreign window
//! geometries or force window focus. The UI relies on the compositor's window
//! placement rules and eframe's standard viewport focus commands.

use crate::popup_focus::FocusRequestState;

/// Resolves initial popup window coordinates under Wayland.
///
/// Always returns `None`, allowing the Wayland compositor to place the popup.
pub fn resolve_popup_position(
    _target_id: Option<&ipc::IpcFocusTarget>,
    _popup_width: f32,
    _popup_height: f32,
    _cursor_offset: f32,
) -> Option<[f32; 2]> {
    None
}

/// Requests native focus under Wayland.
///
/// Returns `FocusRequestState::Unavailable` as direct X11 atom manipulation
/// does not apply to native Wayland surfaces.
pub fn request_focus() -> FocusRequestState {
    FocusRequestState::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_placement_returns_none() {
        assert_eq!(resolve_popup_position(None, 360.0, 420.0, 12.0), None);
    }

    #[test]
    fn wayland_focus_returns_unavailable() {
        assert_eq!(request_focus(), FocusRequestState::Unavailable);
    }
}
