use crate::image_codec::CanonicalImage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardContent {
    Text(String),

    Image(CanonicalImage),
}

impl From<CanonicalImage> for ClipboardContent {
    fn from(value: CanonicalImage) -> Self {
        Self::Image(value)
    }
}

impl From<String> for ClipboardContent {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for ClipboardContent {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<&String> for ClipboardContent {
    fn from(value: &String) -> Self {
        Self::Text(value.clone())
    }
}
