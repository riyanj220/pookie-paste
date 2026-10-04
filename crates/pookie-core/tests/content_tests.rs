use pookie_clipboard::ClipboardContent;
use pookie_core::{ContentAnalyzer, ContentType};

#[test]
fn analyzes_text_content() {
    let content = ClipboardContent::Text("Hello Pookie".to_string());

    let metadata = ContentAnalyzer::analyze(&content);

    assert_eq!(metadata.content_type, ContentType::Text);

    assert_eq!(metadata.size, 12);

    assert!(!metadata.is_empty);
}

#[test]
fn detects_empty_text() {
    let content = ClipboardContent::Text(String::new());

    let metadata = ContentAnalyzer::analyze(&content);

    assert!(metadata.is_empty);
}

#[test]
fn analyzes_image_content() {
    let canonical = pookie_clipboard::canonicalize_rgba(1, 1, &[255, 0, 0, 255]).unwrap();
    let expected_size = canonical.len();
    let content = ClipboardContent::Image(canonical);

    let metadata = ContentAnalyzer::analyze(&content);

    assert_eq!(metadata.content_type, ContentType::Image);
    assert_eq!(metadata.size, expected_size);
    assert!(!metadata.is_empty);
}
