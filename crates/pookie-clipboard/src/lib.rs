//! # Pookie Clipboard
//!
//! Infrastructure layer for clipboard operations in Pookie Paste.
//!
//! This crate defines:
//!
//! - Clipboard backend abstraction
//! - Clipboard content types
//! - Clipboard events
//! - Clipboard-specific errors
//!
//! This crate does not implement platform-specific clipboard logic.
//! X11 and Wayland implementations will use these abstractions.

mod backend;
mod content;
mod error;
mod event;
pub mod x11;


pub use backend::ClipboardBackend;
pub use content::ClipboardContent;
pub use error::ClipboardError;
pub use event::ClipboardEvent;