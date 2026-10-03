use std::borrow::Cow;
use std::sync::Mutex;

use arboard::ImageData;

use crate::{
    ClipboardBackend, ClipboardContent, ClipboardError, canonicalize_rgba,
    decode_canonical_png_to_rgba,
};

pub struct X11Clipboard {
    clipboard: Mutex<arboard::Clipboard>,
    target_reader: Mutex<Option<crate::x11_targets::X11TargetReader>>,
}

impl X11Clipboard {
    pub fn new() -> Result<Self, ClipboardError> {
        let clipboard = arboard::Clipboard::new()
            .map_err(|error| ClipboardError::InitializationFailed(error.to_string()))?;

        let target_reader = match crate::x11_targets::X11TargetReader::new() {
            Ok(reader) => Some(reader),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "failed initializing X11 target reader; falling back to arboard only"
                );
                None
            }
        };

        Ok(Self {
            clipboard: Mutex::new(clipboard),
            target_reader: Mutex::new(target_reader),
        })
    }

    pub fn name(&self) -> &'static str {
        "X11"
    }

    fn read_image_via_arboard(&self) -> Result<ClipboardContent, ClipboardError> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::ReadFailed("clipboard lock poisoned".to_string()))?;

        let image = clipboard
            .get_image()
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

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

        Ok(ClipboardContent::Image(canonical_png))
    }

    fn read_text_via_arboard(&self) -> Result<ClipboardContent, ClipboardError> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::ReadFailed("clipboard lock poisoned".to_string()))?;

        let text = clipboard
            .get_text()
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        tracing::debug!(length = text.len(), "X11 clipboard text read");

        Ok(ClipboardContent::Text(text))
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
        // 1. Existing arboard image attempt is always first and authoritative.
        // This guarantees zero regression for any image format or alias arboard supports.
        if let Ok(image_content) = self.read_image_via_arboard() {
            return Ok(image_content);
        }

        // 2. Direct image was unavailable. Inspect for text/uri-list via X11TargetReader.
        let mut target_reader_guard = self
            .target_reader
            .lock()
            .map_err(|_| ClipboardError::ReadFailed("target reader lock poisoned".to_string()))?;

        if let Some(reader) = target_reader_guard.as_mut() {
            match reader.get_target_capabilities() {
                Ok(Some(caps)) => {
                    // Check text/uri-list when explicitly offered
                    if caps.has_uri_list {
                        match reader.read_uri_list_payload() {
                            Ok(bytes) => {
                                match crate::file_image::canonicalize_single_local_file_uri(&bytes)
                                {
                                    Ok(canonical) => {
                                        tracing::debug!(
                                            encoded_bytes = canonical.len(),
                                            "X11 clipboard file-list image read"
                                        );
                                        return Ok(ClipboardContent::Image(canonical));
                                    }
                                    Err(error) => {
                                        tracing::debug!(
                                            error = %error,
                                            "X11 clipboard URI-list resolution failed; evaluating text fallback"
                                        );
                                    }
                                }
                            }
                            Err(error) => {
                                tracing::debug!(
                                    error = %error,
                                    "X11 clipboard URI-list read failed; evaluating text fallback"
                                );
                            }
                        }

                        // If URI-list failed and no text target was offered, do NOT fabricate text
                        if !caps.has_text {
                            return Err(ClipboardError::ReadFailed(
                                "X11 URI-list does not contain a supported image and no text fallback was offered"
                                    .to_string(),
                            ));
                        }
                    }

                    // 3. Text fallback
                    if caps.has_text {
                        drop(target_reader_guard);
                        return self.read_text_via_arboard();
                    }

                    return Err(ClipboardError::ReadFailed(
                        "no supported clipboard target offered".to_string(),
                    ));
                }

                Ok(None) => {
                    // CLIPBOARD owner is NONE (empty clipboard)
                    return Err(ClipboardError::ReadFailed(
                        "X11 clipboard is empty".to_string(),
                    ));
                }

                Err(error) => {
                    tracing::debug!(
                        error = %error,
                        "X11 target reader query failed; falling back to arboard text"
                    );
                }
            }
        }

        drop(target_reader_guard);
        self.read_text_via_arboard()
    }

    fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
        match content {
            ClipboardContent::Text(text) => self.write(text),

            ClipboardContent::Image(canonical_png) => {
                let (width, height, rgba) = decode_canonical_png_to_rgba(canonical_png.png_bytes())
                    .map_err(|error| {
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
