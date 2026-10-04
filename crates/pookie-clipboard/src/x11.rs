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

    pub fn take_target_reader(&self) -> Option<crate::x11_targets::X11TargetReader> {
        self.target_reader
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    pub fn name(&self) -> &'static str {
        "X11"
    }
}

pub(crate) fn read_image_from_arboard(
    clipboard: &mut arboard::Clipboard,
) -> Result<ClipboardContent, ClipboardError> {
    let image = clipboard
        .get_image()
        .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

    let width = u32::try_from(image.width).map_err(|_| {
        ClipboardError::ReadFailed("X11 clipboard image width exceeds supported range".to_string())
    })?;

    let height = u32::try_from(image.height).map_err(|_| {
        ClipboardError::ReadFailed("X11 clipboard image height exceeds supported range".to_string())
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

pub(crate) fn read_text_from_arboard(
    clipboard: &mut arboard::Clipboard,
) -> Result<ClipboardContent, ClipboardError> {
    let text = clipboard
        .get_text()
        .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

    tracing::debug!(length = text.len(), "X11 clipboard text read");

    Ok(ClipboardContent::Text(text))
}

///
/// Shared authoritative X11 clipboard reading strategy.
///
/// Invariant: Preserves exact precedence across all callers:
/// 1. DirectImage (arboard image)
/// 2. UriList (single local image file via X11TargetReader; stops on failure without text fallback)
/// 3. Text (arboard text; only when text/uri-list is not offered)
///
pub(crate) fn read_x11_content(
    clipboard: &mut arboard::Clipboard,
    target_reader: Option<&mut crate::x11_targets::X11TargetReader>,
    pending_events: &mut std::collections::VecDeque<x11rb::protocol::Event>,
) -> Result<ClipboardContent, ClipboardError> {
    // 1. Existing arboard image attempt is always first and authoritative.
    // This guarantees zero regression for any image format or alias arboard supports.
    if let Ok(image_content) = read_image_from_arboard(clipboard) {
        return Ok(image_content);
    }

    // 2. Direct image was unavailable. Inspect for text/uri-list via X11TargetReader.
    if let Some(reader) = target_reader {
        match reader.get_target_capabilities(pending_events) {
            Ok(Some(caps)) => {
                // If text/uri-list is offered, treat as file-copy operation:
                // must resolve as image or stop (NO fallback to text).
                if caps.has_uri_list {
                    let bytes = reader
                        .read_uri_list_payload(pending_events)
                        .map_err(|error| {
                            ClipboardError::ReadFailed(format!(
                                "failed reading X11 URI-list payload: {error}"
                            ))
                        })?;
                    let canonical = crate::file_image::canonicalize_single_local_file_uri(&bytes)
                        .map_err(|error| {
                        ClipboardError::ReadFailed(format!(
                            "X11 URI-list does not contain a supported image: {error}"
                        ))
                    })?;
                    tracing::debug!(
                        encoded_bytes = canonical.len(),
                        "X11 clipboard file-list image read"
                    );
                    return Ok(ClipboardContent::Image(canonical));
                }

                // 3. Genuine text target (only when URI-list was NOT offered)
                if caps.has_text {
                    return read_text_from_arboard(clipboard);
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
                    "X11 target reader query failed; failing closed to prevent invalid text fallback"
                );
                return Err(ClipboardError::ReadFailed(format!(
                    "failed querying X11 target capabilities: {error}"
                )));
            }
        }
    }

    read_text_from_arboard(clipboard)
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

        let mut target_reader_guard = self
            .target_reader
            .lock()
            .map_err(|_| ClipboardError::ReadFailed("target reader lock poisoned".to_string()))?;

        let mut pending_events = std::collections::VecDeque::new();
        read_x11_content(
            &mut clipboard,
            target_reader_guard.as_mut(),
            &mut pending_events,
        )
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
