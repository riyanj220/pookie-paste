#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardContent {
    Text(String),

    Image(Vec<u8>),
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
