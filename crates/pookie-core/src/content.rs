use pookie_clipboard::ClipboardContent;

#[derive(Debug, Clone, PartialEq)]
pub enum ContentType {
    Text,

    Image,
}

#[derive(Debug, Clone)]
pub struct ContentMetadata {
    pub content_type: ContentType,

    pub size: usize,

    pub is_empty: bool,
}

pub struct ContentAnalyzer;

impl ContentAnalyzer {
    pub fn analyze(content: &ClipboardContent) -> ContentMetadata {
        match content {
            ClipboardContent::Text(text) => ContentMetadata {
                content_type: ContentType::Text,

                size: text.len(),

                is_empty: text.is_empty(),
            },

            ClipboardContent::Image(image) => ContentMetadata {
                content_type: ContentType::Image,

                size: image.len(),

                is_empty: image.is_empty(),
            },
        }
    }
}
