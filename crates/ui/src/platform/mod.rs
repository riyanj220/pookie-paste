//! Platform window placement and focus abstraction for the UI.
//!
//! Provides a unified entry point for window positioning and initial native
//! focus acquisition across X11 and Wayland display environments.

pub mod wayland;
pub mod x11;
pub(crate) use x11::FocusRequestState;

/// Detects whether the current session is running under X11.
fn is_x11_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|s| s.trim().eq_ignore_ascii_case("x11"))
        .unwrap_or(false)
}

/// Resolves initial popup window coordinates based on active display platform.
pub fn resolve_popup_position(
    target_id: Option<&ipc::IpcFocusTarget>,
    popup_width: f32,
    popup_height: f32,
    cursor_offset: f32,
) -> Option<[f32; 2]> {
    if is_x11_session() {
        x11::resolve_popup_position(target_id, popup_width, popup_height, cursor_offset)
    } else {
        wayland::resolve_popup_position(target_id, popup_width, popup_height, cursor_offset)
    }
}

/// Requests native initial focus based on active display platform.
pub fn request_focus() -> FocusRequestState {
    if is_x11_session() {
        x11::request_focus()
    } else {
        wayland::request_focus()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_detection_handles_empty() {
        // Without XDG_SESSION_TYPE=x11, returns false
        assert_eq!(
            std::env::var("XDG_SESSION_TYPE")
                .map(|s| s.trim().eq_ignore_ascii_case("x11"))
                .unwrap_or(false),
            is_x11_session()
        );
    }
}
