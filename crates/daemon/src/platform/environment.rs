//! Desktop session and environment capability auditing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    X11,
    Wayland,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopKind {
    Kde,
    Gnome,
    Wlroots,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentAudit {
    pub session: SessionKind,
    pub desktop: DesktopKind,
    pub raw_session: String,
    pub raw_desktop: String,
}

impl EnvironmentAudit {
    /// Detects environment capabilities from the active process environment.
    pub fn detect() -> Self {
        let raw_session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        let raw_desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        Self::from_env(&raw_session, &raw_desktop)
    }

    /// Pure parser for environment variables, enabling isolated unit testing.
    pub fn from_env(raw_session: &str, raw_desktop: &str) -> Self {
        let session = match raw_session.trim().to_ascii_lowercase().as_str() {
            "x11" => SessionKind::X11,
            "wayland" => SessionKind::Wayland,
            _ => SessionKind::Unknown,
        };

        let desktop_lower = raw_desktop.trim().to_ascii_lowercase();
        let is_match = |name: &str| {
            desktop_lower
                .split([':', ';'])
                .any(|part| part.trim() == name)
        };

        let desktop = if is_match("kde") {
            DesktopKind::Kde
        } else if is_match("gnome") {
            DesktopKind::Gnome
        } else if is_match("sway")
            || is_match("wlroots")
            || is_match("hyprland")
            || is_match("river")
        {
            DesktopKind::Wlroots
        } else {
            DesktopKind::Other
        };

        Self {
            session,
            desktop,
            raw_session: raw_session.to_string(),
            raw_desktop: raw_desktop.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_x11_sessions() {
        let audit = EnvironmentAudit::from_env("x11", "GNOME");
        assert_eq!(audit.session, SessionKind::X11);
        assert_eq!(audit.desktop, DesktopKind::Gnome);

        let audit_kde = EnvironmentAudit::from_env("X11", "KDE");
        assert_eq!(audit_kde.session, SessionKind::X11);
        assert_eq!(audit_kde.desktop, DesktopKind::Kde);
    }

    #[test]
    fn detects_kde_wayland_sessions() {
        let audit = EnvironmentAudit::from_env("wayland", "KDE");
        assert_eq!(audit.session, SessionKind::Wayland);
        assert_eq!(audit.desktop, DesktopKind::Kde);

        let audit_multi = EnvironmentAudit::from_env("wayland", "plasma:kde");
        assert_eq!(audit_multi.session, SessionKind::Wayland);
        assert_eq!(audit_multi.desktop, DesktopKind::Kde);
    }

    #[test]
    fn detects_wlroots_compositors() {
        let audit_sway = EnvironmentAudit::from_env("wayland", "sway");
        assert_eq!(audit_sway.session, SessionKind::Wayland);
        assert_eq!(audit_sway.desktop, DesktopKind::Wlroots);

        let audit_hyprland = EnvironmentAudit::from_env("wayland", "Hyprland");
        assert_eq!(audit_hyprland.session, SessionKind::Wayland);
        assert_eq!(audit_hyprland.desktop, DesktopKind::Wlroots);
    }

    #[test]
    fn handles_unknown_sessions() {
        let audit = EnvironmentAudit::from_env("tty", "none");
        assert_eq!(audit.session, SessionKind::Unknown);
        assert_eq!(audit.desktop, DesktopKind::Other);
    }
}
