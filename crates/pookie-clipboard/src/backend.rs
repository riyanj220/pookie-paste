use crate::{ClipboardContent, ClipboardError};

pub trait ClipboardBackend {
    /*
     * Existing text primitives.
     *
     * These remain temporarily while the X11 and Wayland
     * backends are migrated independently in Phase 10.5
     * and Phase 10.6.
     *
     * ClipboardService no longer depends directly on these
     * methods.
     */
    fn read(&self) -> Result<String, ClipboardError>;

    fn write(&self, content: &str) -> Result<(), ClipboardError>;

    /*
     * Content-aware clipboard API.
     *
     * Text works automatically through the existing backend
     * implementation.
     *
     * Platform backends override these methods when they gain
     * image support.
     */
    fn read_content(&self) -> Result<ClipboardContent, ClipboardError> {
        self.read().map(ClipboardContent::Text)
    }

    fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
        match content {
            ClipboardContent::Text(text) => self.write(text),

            ClipboardContent::Image(_) => Err(ClipboardError::UnsupportedContent(
                "image write is not implemented by this clipboard backend yet".to_string(),
            )),
        }
    }
}
