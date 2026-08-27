use pookie_clipboard::ClipboardContent;

pub struct ContentNormalizer;

impl ContentNormalizer {
    pub fn normalize(content: ClipboardContent) -> ClipboardContent {
        match content {
            ClipboardContent::Text(text) => ClipboardContent::Text(Self::normalize_text(&text)),

            ClipboardContent::Image(image) => ClipboardContent::Image(image),
        }
    }

    fn normalize_text(text: &str) -> String {
        text.replace("\r\n", "\n")
            .replace('\r', "\n")
            .trim()
            .to_string()
    }
}
