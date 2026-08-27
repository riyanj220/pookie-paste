use sha2::{Digest, Sha256};

use pookie_clipboard::ClipboardContent;

pub struct ContentHasher;

impl ContentHasher {
    pub fn hash(content: &ClipboardContent) -> String {
        let bytes = Self::content_bytes(content);

        let mut hasher = Sha256::new();

        hasher.update(bytes);

        let result = hasher.finalize();

        format!("{:x}", result)
    }

    fn content_bytes(content: &ClipboardContent) -> Vec<u8> {
        match content {
            ClipboardContent::Text(text) => text.as_bytes().to_vec(),

            ClipboardContent::Image(image) => image.clone(),
        }
    }
}
