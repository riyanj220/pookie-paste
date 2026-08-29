use pookie_core::{ClipboardPolicy, ContentMetadata, ContentType};

#[test]
fn accepts_content_by_default() {
    let metadata = ContentMetadata {
        content_type: ContentType::Text,

        size: 100,

        is_empty: false,
    };

    assert!(ClipboardPolicy::accept(&metadata));
}

#[test]
fn has_text_size_limit() {
    assert_eq!(ClipboardPolicy::MAX_TEXT_SIZE, 1024 * 1024);
}

#[test]
fn has_image_size_limit() {
    assert_eq!(ClipboardPolicy::MAX_IMAGE_SIZE, 10 * 1024 * 1024);
}

#[test]
fn rejects_empty_content() {
    let metadata = ContentMetadata {
        content_type: ContentType::Text,

        size: 0,

        is_empty: true,
    };

    assert!(!ClipboardPolicy::accept(&metadata));
}

#[test]
fn accepts_small_text() {
    let metadata = ContentMetadata {
        content_type: ContentType::Text,

        size: 500,

        is_empty: false,
    };

    assert!(ClipboardPolicy::accept(&metadata));
}

#[test]
fn rejects_large_text() {
    let metadata = ContentMetadata {
        content_type: ContentType::Text,

        size: ClipboardPolicy::MAX_TEXT_SIZE + 1,

        is_empty: false,
    };

    assert!(!ClipboardPolicy::accept(&metadata));
}

#[test]
fn rejects_large_image() {
    let metadata = ContentMetadata {
        content_type: ContentType::Image,

        size: ClipboardPolicy::MAX_IMAGE_SIZE + 1,

        is_empty: false,
    };

    assert!(!ClipboardPolicy::accept(&metadata));
}

#[test]
fn accepts_text_at_limit() {
    let metadata = ContentMetadata {
        content_type: ContentType::Text,

        size: ClipboardPolicy::MAX_TEXT_SIZE,

        is_empty: false,
    };

    assert!(ClipboardPolicy::accept(&metadata));
}

#[test]
fn accepts_image_at_limit() {
    let metadata = ContentMetadata {
        content_type: ContentType::Image,

        size: ClipboardPolicy::MAX_IMAGE_SIZE,

        is_empty: false,
    };

    assert!(ClipboardPolicy::accept(&metadata));
}

#[test]
fn accepts_small_image() {
    let metadata = ContentMetadata {
        content_type: ContentType::Image,

        size: 1024,

        is_empty: false,
    };

    assert!(ClipboardPolicy::accept(&metadata));
}

#[test]
fn rejects_empty_image() {
    let metadata = ContentMetadata {
        content_type: ContentType::Image,

        size: 0,

        is_empty: true,
    };

    assert!(!ClipboardPolicy::accept(&metadata));
}
