use std::io::Read;

use wl_clipboard_rs::{
    copy::{MimeType as CopyMimeType, Options, Source},
    paste::{ClipboardType, MimeType as PasteMimeType, Seat, get_contents},
};

use crate::{ClipboardBackend, ClipboardError};

#[derive(Debug, Default)]
pub struct WaylandClipboard;

impl WaylandClipboard {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &'static str {
        "Wayland"
    }
}

impl ClipboardBackend for WaylandClipboard {
    fn read(&self) -> Result<String, ClipboardError> {
        /*
         * Clipboard monitoring itself is handled by
         * WaylandClipboardWatcher.
         *
         * This method provides a normal one-shot read for
         * callers using the generic ClipboardBackend API.
         */
        let (mut pipe, mime_type) = get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            PasteMimeType::Text,
        )
        .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        let mut contents = Vec::new();

        pipe.read_to_end(&mut contents)
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        let text = String::from_utf8(contents)
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        tracing::debug!(
            mime_type = %mime_type,
            length = text.len(),
            "Wayland clipboard text read"
        );

        Ok(text)
    }

    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        /*
         * Establish clipboard ownership through Wayland
         * data-control.
         *
         * wl-clipboard-rs handles EXT/WLR protocol
         * selection internally and serves future clipboard
         * requests after this method returns.
         */
        let source = Source::Bytes(content.as_bytes().to_vec().into());

        Options::new()
            .copy(source, CopyMimeType::Text)
            .map_err(|error| ClipboardError::WriteFailed(error.to_string()))?;

        tracing::debug!(
            length = content.len(),
            "Wayland clipboard ownership established"
        );

        Ok(())
    }
}
