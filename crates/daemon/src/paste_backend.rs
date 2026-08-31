#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteCapability {
    Direct,
    ClipboardOnly,
}

#[derive(Debug)]
pub enum PasteError {
    Unavailable,
    Failed(String),
}

pub trait PasteBackend: Send + Sync {
    fn capability(&self) -> PasteCapability;

    fn paste(&self) -> Result<(), PasteError>;
}

pub struct ClipboardOnlyPasteBackend;

impl PasteBackend for ClipboardOnlyPasteBackend {
    fn capability(&self) -> PasteCapability {
        PasteCapability::ClipboardOnly
    }

    fn paste(&self) -> Result<(), PasteError> {
        Ok(())
    }
}
