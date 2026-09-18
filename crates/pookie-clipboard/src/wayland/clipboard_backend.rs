use std::io::Read;

use wl_clipboard_rs::{
    copy::{MimeType as CopyMimeType, Options, Source},
    paste::{ClipboardType, MimeType as PasteMimeType, Seat, get_contents, get_mime_types_ordered},
};

use crate::{
    ClipboardBackend, ClipboardContent, ClipboardError, canonicalize_image, preferred_image_mime,
};

#[derive(Debug, Default)]
pub struct WaylandClipboard;

impl WaylandClipboard {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &'static str {
        "Wayland"
    }

    fn read_pipe_bytes(&self, mut pipe: impl Read) -> Result<Vec<u8>, ClipboardError> {
        let mut contents = Vec::new();

        pipe.read_to_end(&mut contents)
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        Ok(contents)
    }
}

impl ClipboardBackend for WaylandClipboard {
    fn read(&self) -> Result<String, ClipboardError> {
        /*
         * Preserve the existing one-shot text read.
         */
        let (pipe, mime_type) = get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            PasteMimeType::Text,
        )
        .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        let contents = self.read_pipe_bytes(pipe)?;

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
         * Preserve the existing text ownership path.
         */
        let source = Source::Bytes(content.as_bytes().to_vec().into());

        Options::new()
            .copy(source, CopyMimeType::Text)
            .map_err(|error| ClipboardError::WriteFailed(error.to_string()))?;

        tracing::debug!(
            length = content.len(),
            "Wayland clipboard text ownership established"
        );

        Ok(())
    }

    fn read_content(&self) -> Result<ClipboardContent, ClipboardError> {
        /*
         * Enumerate the full offer first.
         *
         * wl-clipboard-rs otherwise prioritizes text for
         * MimeType::Any, which is the opposite of Pookie's
         * desired behavior when an application offers both
         * image data and a text fallback.
         */
        let offered = get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified)
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        if let Some(image_mime) = preferred_image_mime(&offered) {
            let (pipe, actual_mime) = get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                PasteMimeType::Specific(image_mime),
            )
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

            let bytes = self.read_pipe_bytes(pipe)?;

            let canonical = canonicalize_image(&bytes, &actual_mime).map_err(|error| {
                ClipboardError::ReadFailed(format!(
                    "failed canonicalizing Wayland clipboard image: {error}"
                ))
            })?;

            tracing::debug!(
                mime_type = %actual_mime,
                encoded_bytes =
                canonical.len(),
                            "Wayland clipboard image read"
            );

            return Ok(ClipboardContent::Image(canonical));
        }

        self.read().map(ClipboardContent::Text)
    }

    fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
        match content {
            ClipboardContent::Text(text) => self.write(text),

            ClipboardContent::Image(canonical_png) => {
                /*
                 * ClipboardContent::Image already means
                 * canonical PNG by the Phase 10 invariant.
                 *
                 * Wayland therefore only needs to expose
                 * image/png on writeback.
                 */
                let source = Source::Bytes(canonical_png.clone().into());

                Options::new()
                    .copy(source, CopyMimeType::Specific("image/png".to_string()))
                    .map_err(|error| ClipboardError::WriteFailed(error.to_string()))?;

                tracing::debug!(
                    encoded_bytes = canonical_png.len(),
                    "Wayland clipboard image ownership established"
                );

                Ok(())
            }
        }
    }
}
