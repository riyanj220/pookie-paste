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
pub(crate) mod file_image;
pub mod image_codec;
pub(crate) mod uri_list;
mod watcher;

pub mod wayland;

pub mod x11;
pub(crate) mod x11_targets;
pub mod x11_watcher;

pub use backend::ClipboardBackend;
pub use content::ClipboardContent;
pub use error::ClipboardError;
pub use event::ClipboardEvent;

pub use image_codec::{
    CanonicalImage, ImageCodecError, ImageIdentity, MAX_DECODE_ALLOCATION, MAX_IMAGE_DIMENSION,
    MAX_IMAGE_PIXELS, RGBA_V1_DOMAIN_SEPARATOR, SUPPORTED_IMAGE_MIME_TYPES, canonicalize_image,
    canonicalize_rgba, compute_image_identity, decode_canonical_png_to_rgba,
    is_supported_image_mime, preferred_image_mime,
};

pub use watcher::ClipboardWatcher;

pub use wayland::WaylandClipboardWatcher;
pub use x11_watcher::X11ClipboardWatcher;
