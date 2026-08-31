use std::sync::Mutex;

use crate::{ClipboardBackend, ClipboardError};

pub struct X11Clipboard {
    clipboard: Mutex<arboard::Clipboard>,
}

impl X11Clipboard {
    pub fn new() -> Result<Self, ClipboardError> {
        let clipboard = arboard::Clipboard::new()
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))?;

        Ok(Self {
            clipboard: Mutex::new(clipboard),
        })
    }

    pub fn name(&self) -> &'static str {
        "X11"
    }
}

impl ClipboardBackend for X11Clipboard {
    fn read(&self) -> Result<String, ClipboardError> {
        let mut clipboard = self
            .clipboard
            .lock()
            .map_err(|_| ClipboardError::ReadFailed("clipboard lock poisoned".to_string()))?;

        clipboard
            .get_text()
            .map_err(|error| ClipboardError::ReadFailed(error.to_string()))
    }

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
}
