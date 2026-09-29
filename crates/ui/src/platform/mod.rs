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
/// from raw `XDG_SESSION_TYPE`, `XDG_CURRENT_DESKTOP`, and validated compositor runtime signals.
///
/// This function is deliberately free of environment variable and filesystem access so that
/// it can be tested in parallel without process-global mutation.
pub fn resolve_focus_loss_policy(
    raw_session: &str,
    raw_desktop: &str,
    sway_runtime_available: bool,
    hyprland_runtime_available: bool,
) -> FocusLossDismissalPolicy {
    let session = raw_session.trim().to_ascii_lowercase();
    let desktop = raw_desktop.trim().to_ascii_lowercase();

    /*
     * 1. X11 session takes precedence.
     * X11 window managers default to click-to-focus.
     * Focus loss is 1:1 with an intentional user action.
     */
    if session == "x11" {
        return FocusLossDismissalPolicy::Dismiss;
    }

    /*
     * 2. KDE Plasma (Wayland) defaults to click-to-focus.
     * KWin does not emit wl_keyboard.leave on pointer motion.
     * Evaluated before secondary sockets to ensure stale socket variables
     * do not override an explicitly detected KDE session.
     */
    if desktop_contains(&desktop, "kde") {
        return FocusLossDismissalPolicy::Dismiss;
    }

    /*
     * 3. Sway desktop token.
     * Sway defaults to focus_follows_mouse yes.
     * Pointer motion across window boundaries immediately reallocates
     * keyboard focus, so focus loss is not a reliable dismissal signal.
     */
    if desktop_contains(&desktop, "sway") {
        return FocusLossDismissalPolicy::Ignore;
    }

    /*
     * 4. Hyprland desktop token.
     * Hyprland defaults to follow_mouse = 1.
     * Same focus-follows-mouse semantics as Sway.
     */
    if desktop_contains(&desktop, "hyprland") {
        return FocusLossDismissalPolicy::Ignore;
    }

    /*
     * 5. Validated Sway runtime signal.
     * In environments where XDG_CURRENT_DESKTOP is generic (e.g. wlroots)
     * or unset, a valid SWAYSOCK confirms a live Sway session.
     */
    if sway_runtime_available {
        return FocusLossDismissalPolicy::Ignore;
    }

    /*
     * 6. Validated Hyprland runtime signal.
     * Valid HYPRLAND_INSTANCE_SIGNATURE command socket confirms a live Hyprland session.
     */
    if hyprland_runtime_available {
        return FocusLossDismissalPolicy::Ignore;
    }

    /*
     * 7. Unknown or unsupported compositor: preserve the existing behavior
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
    let sway_runtime = is_sway_socket_valid();
    let hyprland_runtime = is_hyprland_socket_valid();
    resolve_focus_loss_policy(&session, &desktop, sway_runtime, hyprland_runtime)
}

/// Validates whether the Sway IPC socket exists at `$SWAYSOCK`.
fn is_sway_socket_valid() -> bool {
    std::env::var_os("SWAYSOCK")
        .map(std::path::PathBuf::from)
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Validates whether the Hyprland command socket exists using `$HYPRLAND_INSTANCE_SIGNATURE`.
fn is_hyprland_socket_valid() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
        .map(|sig| {
            if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
                let candidate = std::path::PathBuf::from(xdg)
                    .join("hypr")
                    .join(&sig)
                    .join(".socket.sock");
                if candidate.exists() {
                    return true;
                }
            }
            let fallback = std::path::PathBuf::from("/tmp/hypr")
                .join(sig)
                .join(".socket.sock");
            fallback.exists()
        })
        .unwrap_or(false)
}

/// Returns true if a composite desktop string contains the given name.
///
/// Splits on both ':' and ';', trims whitespace from each token, and
/// checks for an exact, case-insensitive match against `name`.
fn desktop_contains(desktop_lower: &str, name: &str) -> bool {
    desktop_lower
        .split([':', ';'])
        .any(|part| part.trim() == name)
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
            resolve_focus_loss_policy("x11", "", false, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn x11_session_is_case_insensitive() {
        assert_eq!(
            resolve_focus_loss_policy("X11", "", false, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn x11_with_sway_and_hyprland_runtime_signals_uses_dismiss() {
        // Even if secondary sockets are active, an X11 session must dismiss.
        assert_eq!(
            resolve_focus_loss_policy("x11", "sway", true, true),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn kde_wayland_uses_dismiss_policy() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "KDE", false, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn plasma_colon_kde_desktop_uses_dismiss() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "plasma:kde", false, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn plasma_semicolon_kde_desktop_uses_dismiss() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "plasma;kde", false, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn kde_with_stale_sway_runtime_signal_uses_dismiss() {
        // KDE precedence prevents leaked secondary socket variables from altering policy.
        assert_eq!(
            resolve_focus_loss_policy("wayland", "kde", true, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn sway_desktop_uses_ignore_policy() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "sway", false, false),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn colon_separated_desktop_detects_sway() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "sway:wlroots", false, false),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn semicolon_separated_desktop_detects_sway() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "sway;wlroots", false, false),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn hyprland_desktop_uses_ignore_policy() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "Hyprland", false, false),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn colon_separated_desktop_detects_hyprland() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "hyprland:wlroots", false, false),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn semicolon_separated_desktop_detects_hyprland() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "hyprland;wlroots", false, false),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn empty_desktop_with_valid_sway_runtime_signal_uses_ignore() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "", true, false),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn empty_desktop_with_valid_hyprland_runtime_signal_uses_ignore() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "", false, true),
            FocusLossDismissalPolicy::Ignore,
        );
    }

    #[test]
    fn unknown_compositor_without_runtime_signals_uses_dismiss() {
        assert_eq!(
            resolve_focus_loss_policy("wayland", "gnome", false, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }

    #[test]
    fn empty_environment_without_runtime_signals_uses_dismiss() {
        assert_eq!(
            resolve_focus_loss_policy("", "", false, false),
            FocusLossDismissalPolicy::Dismiss,
        );
    }
}
