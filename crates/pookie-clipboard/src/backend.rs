use crate::ClipboardError;

pub trait ClipboardBackend {
    fn read(&self) -> Result<String, ClipboardError>;

    fn write(&self, content: &str) -> Result<(), ClipboardError>;
}
