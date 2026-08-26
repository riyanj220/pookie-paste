//! # Pookie Clipboard
//!
//! Clipboard infrastructure layer for Pookie Paste.
//!
//! This crate provides:
//!
//! - Platform-independent clipboard abstraction
//! - Clipboard backend interface
//! - Clipboard-specific error handling
//!
//! Implementations such as X11 and Wayland backends
//! will live behind the [`ClipboardBackend`] trait.

mod backend;
mod error;


pub use backend::ClipboardBackend;
pub use error::ClipboardError;