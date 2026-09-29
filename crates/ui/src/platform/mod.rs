//! Platform window placement and focus abstraction for the UI.
//!
//! Provides a unified entry point for window positioning, initial native
//! focus acquisition, and popup dismissal policy across X11 and Wayland
//! display environments.

pub mod wayland;
pub mod x11;
pub(crate) use x11::FocusRequestState;

/// Controls whether keyboard focus loss is treated as a dismissal signal.
///
/// On click-to-focus environments (X11, KDE Plasma), keyboard focus loss
/// reliably indicates that the user clicked another window. On
/// focus-follows-mouse environments (Sway, Hyprland), keyboard focus can
/// transfer to another window merely from pointer motion, so focus loss
/// alone is an unreliable dismissal signal.
///
/// Resolved once during application initialization and stored in app state.
/// Never re-read from the environment inside the egui update loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusLossDismissalPolicy {
    /// Focus loss closes the popup on environments where Pookie currently
    /// treats focus loss as a reliable dismissal signal.
    Dismiss,

    /// Focus loss does not close the popup. Used on Sway and Hyprland, where focus loss may be caused by
    /// pointer-driven compositor focus changes rather than dismissal intent.
    Ignore,
}

/// Pure policy resolver. Determines the correct focus-loss dismissal policy
/// from raw `XDG_SESSION_TYPE` and `XDG_CURRENT_DESKTOP` values.
///
/// This function is deliberately free of environment variable access so that
/// it can be tested in parallel without process-global mutation.
pub fn resolve_focus_loss_policy(raw_session: &str, raw_desktop: &str) -> FocusLossDismissalPolicy {
    let session = raw_session.trim().to_ascii_lowercase();
    let desktop = raw_desktop.trim().to_ascii_lowercase();

    /*
     * X11 window managers default to click-to-focus.
     * Focus loss is 1:1 with an intentional user action.
     */
    if session == "x11" {
        return FocusLossDismissalPolicy::Dismiss;
    }

    /*
     * KDE Plasma (Wayland) defaults to click-to-focus.
     * KWin does not emit wl_keyboard.leave on pointer motion.
     */
    if desktop_contains(&desktop, "kde") {
        return FocusLossDismissalPolicy::Dismiss;
    }

    /*
     * Sway defaults to focus_follows_mouse yes.
     * Pointer motion across window boundaries immediately reallocates
     * keyboard focus, so focus loss is not a reliable dismissal signal.
     */
    if desktop_contains(&desktop, "sway") {
        return FocusLossDismissalPolicy::Ignore;
    }

    /*
     * Hyprland defaults to follow_mouse = 1.
     * Same focus-follows-mouse semantics as Sway.
     */
    if desktop_contains(&desktop, "hyprland") {
        return FocusLossDismissalPolicy::Ignore;
    }

    /*
     * Unknown or unsupported compositor: preserve the existing behavior
     * (dismiss on focus loss) rather than silently changing it on an
     * untested environment.
     */
    FocusLossDismissalPolicy::Dismiss
}

/// Reads the actual process environment once and returns the resolved policy.
///
/// Call exactly once during application initialization and store the result
/// in app state. Do not call from the egui update loop.
pub fn focus_loss_dismissal_policy() -> FocusLossDismissalPolicy {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    resolve_focus_loss_policy(&session, &desktop)
}

/// Returns true if a colon-separated desktop string contains the given name.
///
/// `XDG_CURRENT_DESKTOP` may be colon-separated (e.g. `"sway:wlroots"`).
fn desktop_contains(desktop_lower: &str, name: &str) -> bool {
    desktop_lower.split(':').any(|part| part.trim() == name)
}

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
        // Without XDG_SESSION_TYPE=x11, is_x11_session() follows the env.
        assert_eq!(
            std::env::var("XDG_SESSION_TYPE")
                .map(|s| s.trim().eq_ignore_ascii_case("x11"))
                .unwrap_or(false),
            is_x11_session()
        );
    }

    // ---------------------------------------------------------------
    // Focus-loss dismissal policy tests.
    //
    // All tests use the pure resolver with explicit inputs so that
    // they are free of process-global environment mutation and are
    // safe to run in parallel.
    // ---------------------------------------------------------------

    #[test]
    fn x11_session_uses_dismiss_policy() {
        assert_eq!(
            resolve_focus_loss_policy("x11", ""),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn x11_session_is_case_insensitive() {
        assert_eq!(
            resolve_focus_loss_policy("X11", ""),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn kde_desktop_uses_dismiss_policy() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "KDE"),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn sway_desktop_uses_ignore_policy() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "sway"),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn hyprland_desktop_uses_ignore_policy() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "Hyprland"),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn colon_separated_desktop_detects_sway() {
        // XDG_CURRENT_DESKTOP may be colon-separated.
        assert_eq!(
            resolve_focus_loss_policy("wayland", "sway:wlroots"),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn unknown_compositor_preserves_existing_dismiss_policy() {
        // Unknown Wayland compositors keep the original behavior rather than
        // silently changing dismissal on untested environments.
        assert_eq!(
            resolve_focus_loss_policy("wayland", "gnome"),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn empty_environment_preserves_existing_dismiss_policy() {
        assert_eq!(
            resolve_focus_loss_policy("", ""),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn x11_session_overrides_sway_desktop() {
        // If XDG_SESSION_TYPE reports x11, trust the session type regardless
        // of what XDG_CURRENT_DESKTOP says.
        assert_eq!(
            resolve_focus_loss_policy("x11", "sway"),
            FocusLossDismissalPolicy::Dismiss,
        );
    }
}
