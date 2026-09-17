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
//! - Clipboard image canonicalization
//!
//! Platform-specific X11 and Wayland implementations
//! use these abstractions.

mod backend;
mod content;
mod error;
mod event;
pub mod image_codec;
mod watcher;

pub mod wayland;

pub mod x11;
pub mod x11_watcher;

pub use backend::ClipboardBackend;
pub use content::ClipboardContent;
pub use error::ClipboardError;
pub use event::ClipboardEvent;

pub use image_codec::{
    ImageCodecError, MAX_DECODE_ALLOCATION, MAX_IMAGE_DIMENSION, MAX_IMAGE_PIXELS,
    SUPPORTED_IMAGE_MIME_TYPES, canonicalize_image, is_supported_image_mime, preferred_image_mime,
};

pub use watcher::ClipboardWatcher;

pub use wayland::WaylandClipboardWatcher;
pub use x11_watcher::X11ClipboardWatcher;
