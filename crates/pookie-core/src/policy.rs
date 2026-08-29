use crate::ContentMetadata;

pub struct ClipboardPolicy;

impl ClipboardPolicy {
    pub const MAX_TEXT_SIZE: usize = 1024 * 1024;

    pub const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

    pub fn accept(metadata: &ContentMetadata) -> bool {
        if metadata.is_empty {
            return false;
        }

        match metadata.content_type {
            crate::ContentType::Text => metadata.size <= Self::MAX_TEXT_SIZE,

            crate::ContentType::Image => metadata.size <= Self::MAX_IMAGE_SIZE,
        }
    }
}
