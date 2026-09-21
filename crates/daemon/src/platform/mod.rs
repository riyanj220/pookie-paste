//! Platform capability auditing and backend resolution module.
//!
//! Provides capability-specific resolvers that inspect the desktop environment
//! and select existing verified backends for clipboard, focus, paste, and shortcuts.

pub mod environment;
pub mod resolvers;

pub use environment::{DesktopKind, EnvironmentAudit, SessionKind};
pub use resolvers::{
    resolve_clipboard_backend, resolve_focus_backend, resolve_paste_backend,
    resolve_shortcut_backend,
};
