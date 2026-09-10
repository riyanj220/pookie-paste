use crate::paste_backend::{PasteBackend, PasteCapability, PasteError};

#[derive(Debug, Default)]
pub struct WaylandPasteBackend;

impl WaylandPasteBackend {
    pub fn new() -> Self {
        Self
    }
}

impl PasteBackend for WaylandPasteBackend {
    fn capability(&self) -> PasteCapability {
        PasteCapability::ClipboardOnly
    }

    fn paste(&self) -> Result<(), PasteError> {
        Err(PasteError::Unavailable)
    }
}
