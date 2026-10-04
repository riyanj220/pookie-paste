use sha2::{Digest, Sha256};

use pookie_clipboard::ClipboardContent;

pub struct ContentHasher;

impl ContentHasher {
    pub fn hash(content: &ClipboardContent) -> String {
        match content {
            ClipboardContent::Text(text) => {
                let mut hasher = Sha256::new();
                hasher.update(text.as_bytes());
                let result = hasher.finalize();
                format!("{:x}", result)
            }

            ClipboardContent::Image(image) => image.identity().to_versioned_string(),
        }
    }
}
