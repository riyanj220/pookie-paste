use std::borrow::Cow;
use std::sync::Mutex;

use arboard::ImageData;

use crate::{
    ClipboardBackend, ClipboardContent, ClipboardError, canonicalize_rgba,
    decode_canonical_png_to_rgba,
};

pub struct X11Clipboard {
    clipboard: Mutex<arboard::Clipboard>,
}

impl X11Clipboard {
    pub fn new() -> Result<Self, ClipboardError> {
        let clipboard = arboard::Clipboard::new()
            .map_err(|error| ClipboardError::InitializationFailed(error.to_string()))?;

        Ok(Self {
            clipboard: Mutex::new(clipboard),
        })
    }

    pub fn name(&self) -> &'static str {
        "X11"
    }
}

impl ClipboardBackend for X11Clipboard {
    /*
     * Legacy text-only primitive retained during the
     * Phase 10 backend migration.
     */
    fn read(&self) -> Result<String, ClipboardError> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::ReadFailed("clipboard lock poisoned".to_string()))?;

        clipboard
            .get_text()
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))
    }

    /*
     * Legacy text-only primitive retained during the
     * Phase 10 backend migration.
     */
    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::WriteFailed("clipboard lock poisoned".to_string()))?;

        clipboard
            .set_text(content)
            .map_err(|error| ClipboardError::WriteFailed(error.to_string()))?;

        Ok(())
    }

    fn read_content(&self) -> Result<ClipboardContent, ClipboardError> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::ReadFailed("clipboard lock poisoned".to_string()))?;

        /*
         * Prefer a real clipboard image when one is
         * available.
         *
         * Applications such as browsers may expose both an
         * image and textual fallback data for the same copy
         * operation. Pookie should treat that as an image.
         */
        if let Ok(image) = clipboard.get_image() {
            let width = u32::try_from(image.width).map_err(|_| {
                ClipboardError::ReadFailed(
                    "X11 clipboard image width exceeds supported range".to_string(),
                )
            })?;

            let height = u32::try_from(image.height).map_err(|_| {
                ClipboardError::ReadFailed(
                    "X11 clipboard image height exceeds supported range".to_string(),
                )
            })?;

            let canonical_png =
                canonicalize_rgba(width, height, image.bytes.as_ref()).map_err(|error| {
                    ClipboardError::ReadFailed(format!(
                        "failed canonicalizing X11 clipboard image: {error}"
                    ))
                })?;

            tracing::debug!(
                width,
                height,
                encoded_bytes = canonical_png.len(),
                "X11 clipboard image read"
            );

            return Ok(ClipboardContent::Image(canonical_png));
        }

        /*
         * No supported image was available.
         *
         * Preserve the existing text path exactly as the
         * fallback.
         */
        let text = clipboard
            .get_text()
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        tracing::debug!(length = text.len(), "X11 clipboard text read");

        Ok(ClipboardContent::Text(text))
    }

    fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
        match content {
            ClipboardContent::Text(text) => self.write(text),

            ClipboardContent::Image(canonical_png) => {
                let (width, height, rgba) =
                    decode_canonical_png_to_rgba(canonical_png).map_err(|error| {
                        ClipboardError::WriteFailed(format!(
                            "failed decoding canonical image for X11 clipboard: {error}"
                        ))
                    })?;

                let width = usize::try_from(width).map_err(|_| {
                    ClipboardError::WriteFailed(
                        "X11 image width exceeds platform usize".to_string(),
                    )
                })?;

                let height = usize::try_from(height).map_err(|_| {
                    ClipboardError::WriteFailed(
                        "X11 image height exceeds platform usize".to_string(),
                    )
                })?;

                let mut clipboard = self.clipboard.lock().map_err(|_| {
                    ClipboardError::WriteFailed("clipboard lock poisoned".to_string())
                })?;

                clipboard
                    .set_image(ImageData {
                        width,
                        height,
                        bytes: Cow::Owned(rgba),
                    })
                    .map_err(|error| ClipboardError::WriteFailed(error.to_string()))?;

                tracing::debug!(
                    width,
                    height,
                    encoded_bytes = canonical_png.len(),
                    "X11 clipboard image ownership established"
                );

                Ok(())
            }
        }
    }
}
