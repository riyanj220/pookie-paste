use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTheme {
    Light,
    Dark,
}

pub fn detect_system_theme() -> AppTheme {
    detect_portal_theme()
        .or_else(detect_gtk_override)
        .unwrap_or(AppTheme::Dark)
}

fn detect_portal_theme() -> Option<AppTheme> {
    detect_with_gdbus().or_else(detect_with_busctl)
}

fn detect_with_gdbus() -> Option<AppTheme> {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;

    parse_portal_theme(&stdout)
}

fn detect_with_busctl() -> Option<AppTheme> {
    let output = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "Read",
            "ss",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;

    parse_portal_theme(&stdout)
}

fn parse_portal_theme(output: &str) -> Option<AppTheme> {
    let values = output.split(|character: char| !character.is_ascii_alphanumeric());

    let mut saw_uint32 = false;

    for value in values {
        match value {
            "uint32" | "u" => {
                saw_uint32 = true;
            }

            "1" if saw_uint32 => {
                return Some(AppTheme::Dark);
            }

            "2" if saw_uint32 => {
                return Some(AppTheme::Light);
            }

            "0" if saw_uint32 => {
                return None;
            }

            _ => {}
        }
    }

    None
}

fn detect_gtk_override() -> Option<AppTheme> {
    let theme = std::env::var("GTK_THEME").ok()?.to_ascii_lowercase();

    if theme.contains("dark") {
        Some(AppTheme::Dark)
    } else if theme.contains("light") {
        Some(AppTheme::Light)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gdbus_dark_theme() {
        assert_eq!(parse_portal_theme("(<<uint32 1>>,),"), Some(AppTheme::Dark),);
    }

    #[test]
    fn parses_gdbus_light_theme() {
        assert_eq!(
            parse_portal_theme("(<<uint32 2>>,),"),
            Some(AppTheme::Light),
        );
    }

    #[test]
    fn parses_no_preference() {
        assert_eq!(parse_portal_theme("(<<uint32 0>>,),"), None,);
    }

    #[test]
    fn parses_busctl_dark_theme() {
        assert_eq!(parse_portal_theme("v u 1"), Some(AppTheme::Dark),);
    }

    #[test]
    fn parses_busctl_light_theme() {
        assert_eq!(parse_portal_theme("v u 2"), Some(AppTheme::Light),);
    }

    #[test]
    fn rejects_unrelated_output() {
        assert_eq!(parse_portal_theme("something else"), None,);
    }
}
