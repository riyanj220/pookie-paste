//! Capability-specific resolvers that select existing platform backends based on environment audit.

use anyhow::Result;
use std::sync::Arc;

use pookie_clipboard::{wayland::WaylandClipboard, x11::X11Clipboard};

use super::environment::{DesktopKind, EnvironmentAudit, SessionKind};
use crate::clipboard_backend::PlatformClipboard;
use crate::focus_backend::{FocusError, UnavailableFocusBackend};
use crate::hyprland_focus_backend::HyprlandFocusBackend;
use crate::kde_focus_backend::KdeFocusBackend;
use crate::paste_backend::{PasteError, PlatformPasteBackend, WaylandPasteBackend};
use crate::platform_focus_backend::PlatformFocusBackend;
use crate::platform_shortcut_backend::PlatformShortcutBackend;
use crate::portal_eis_paste_backend::PortalEisPasteBackend;
use crate::shortcut_backend::ShortcutError;
use crate::sway_focus_backend::SwayFocusBackend;
use crate::wayland_shortcut_backend::WaylandShortcutBackend;
use crate::wlroots_paste_backend::WlrootsPasteBackend;
use crate::x11_focus_backend::X11FocusBackend;
use crate::x11_paste_backend::X11PasteBackend;
use crate::x11_shortcut_backend::X11ShortcutBackend;

/// Selects the appropriate clipboard provider backend based on environment audit.
pub fn resolve_clipboard_backend(audit: &EnvironmentAudit) -> Result<PlatformClipboard> {
    match audit.session {
        SessionKind::Wayland => Ok(PlatformClipboard::Wayland(
            Arc::new(WaylandClipboard::new()),
        )),
        _ => Ok(PlatformClipboard::X11(Arc::new(X11Clipboard::new()?))),
    }
}

/// Selects the appropriate focus capture and restoration backend based on environment audit.
pub fn resolve_focus_backend(audit: &EnvironmentAudit) -> Result<PlatformFocusBackend, FocusError> {
    match audit.session {
        SessionKind::X11 => Ok(PlatformFocusBackend::X11(Box::new(X11FocusBackend::new()?))),

        SessionKind::Wayland => match audit.desktop {
            DesktopKind::Kde => match KdeFocusBackend::new() {
                Ok(backend) => Ok(PlatformFocusBackend::Kde(backend)),
                Err(error) => {
                    tracing::warn!(
                        error = ?error,
                        "KDE focus helper unavailable; using focus fallback"
                    );
                    Ok(PlatformFocusBackend::Unavailable(UnavailableFocusBackend))
                }
            },
            _ => {
                if std::env::var_os("SWAYSOCK").is_some() {
                    match SwayFocusBackend::new() {
                        Ok(backend) => Ok(PlatformFocusBackend::Sway(backend)),
                        Err(error) => {
                            tracing::warn!(
                                error = ?error,
                                "Sway focus backend unavailable; using focus fallback"
                            );
                            Ok(PlatformFocusBackend::Unavailable(UnavailableFocusBackend))
                        }
                    }
                } else if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
                    match HyprlandFocusBackend::new() {
                        Ok(backend) => Ok(PlatformFocusBackend::Hyprland(backend)),
                        Err(error) => {
                            tracing::warn!(
                                error = ?error,
                                "Hyprland focus backend unavailable; using focus fallback"
                            );
                            Ok(PlatformFocusBackend::Unavailable(UnavailableFocusBackend))
                        }
                    }
                } else {
                    Ok(PlatformFocusBackend::Unavailable(UnavailableFocusBackend))
                }
            }
        },

        SessionKind::Unknown => Ok(PlatformFocusBackend::Unavailable(UnavailableFocusBackend)),
    }
}

/// Selects the paste backend based on environment audit and focus restoration capability.
pub fn resolve_paste_backend(
    audit: &EnvironmentAudit,
    allow_wayland_direct: bool,
) -> Result<PlatformPasteBackend, PasteError> {
    match audit.session {
        SessionKind::X11 => Ok(PlatformPasteBackend::X11(Box::new(X11PasteBackend::new()?))),

        SessionKind::Wayland => {
            if !allow_wayland_direct {
                tracing::info!(
                    "Wayland direct paste disabled because focus restoration is unavailable"
                );
                return Ok(PlatformPasteBackend::WaylandFallback(
                    WaylandPasteBackend::new(),
                ));
            }

            match audit.desktop {
                DesktopKind::Kde => match PortalEisPasteBackend::new() {
                    Ok(backend) => Ok(PlatformPasteBackend::WaylandDirect(backend)),
                    Err(error) => {
                        tracing::warn!(
                            error = ?error,
                            "Portal/EIS direct paste unavailable; using clipboard-only fallback"
                        );
                        Ok(PlatformPasteBackend::WaylandFallback(
                            WaylandPasteBackend::new(),
                        ))
                    }
                },
                _ => {
                    let is_sway = std::env::var_os("SWAYSOCK").is_some();
                    let is_hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();

                    if is_sway || is_hyprland {
                        match WlrootsPasteBackend::new() {
                            Ok(backend) => Ok(PlatformPasteBackend::WaylandWlroots(backend)),
                            Err(error) => {
                                tracing::warn!(
                                    error = ?error,
                                    "wlroots virtual keyboard direct paste unavailable; using clipboard-only fallback"
                                );
                                Ok(PlatformPasteBackend::WaylandFallback(
                                    WaylandPasteBackend::new(),
                                ))
                            }
                        }
                    } else {
                        tracing::info!(
                            "Unverified Wayland compositor; using clipboard-only fallback"
                        );
                        Ok(PlatformPasteBackend::WaylandFallback(
                            WaylandPasteBackend::new(),
                        ))
                    }
                }
            }
        }

        SessionKind::Unknown => Ok(PlatformPasteBackend::WaylandFallback(
            WaylandPasteBackend::new(),
        )),
    }
}

/// Selects the global shortcut registration backend based on environment audit.
pub fn resolve_shortcut_backend(
    audit: &EnvironmentAudit,
) -> Result<PlatformShortcutBackend, ShortcutError> {
    match audit.session {
        SessionKind::X11 => Ok(PlatformShortcutBackend::X11(Box::new(
            X11ShortcutBackend::new()?,
        ))),

        SessionKind::Wayland => Ok(PlatformShortcutBackend::Wayland(Box::new(
            WaylandShortcutBackend::new()?,
        ))),

        SessionKind::Unknown => Ok(PlatformShortcutBackend::Unavailable),
    }
}
