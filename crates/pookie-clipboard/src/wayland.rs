use crate::{ClipboardBackend, ClipboardError};

pub struct WaylandClipboard {}

impl WaylandClipboard {
    pub fn new() -> Result<Self, ClipboardError> {
        Ok(Self {})
    }

    pub fn name(&self) -> &'static str {
        "Wayland"
    }
}

impl ClipboardBackend for WaylandClipboard {
    fn read(&self) -> Result<String, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(|error| {
            tracing::warn!("Wayland clipboard initialization failed: {:?}", error);
            ClipboardError::ReadFailed(error.to_string())
        })?;

        match clipboard.get_text() {
            Ok(text) => {
                tracing::info!("Wayland clipboard read: {:?}", text);

                Ok(text)
            }

            Err(error) => {
                tracing::warn!("Wayland clipboard read failed: {:?}", error);

                Err(ClipboardError::ReadFailed(error.to_string()))
            }
        }
    }

    fn write(&self, content: &str) -> Result<(), ClipboardError> {
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|error| ClipboardError::WriteFailed(error.to_string()))?;

        clipboard
            .set_text(content)
            .map_err(|error| ClipboardError::WriteFailed(error.to_string()))?;

        Ok(())
    }
}
