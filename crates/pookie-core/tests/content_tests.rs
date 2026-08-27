use pookie_core::{ContentAnalyzer, ContentType};

use pookie_clipboard::ClipboardContent;

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
    let content = ClipboardContent::Image(vec![1, 2, 3, 4]);

    let metadata = ContentAnalyzer::analyze(&content);

    assert_eq!(metadata.content_type, ContentType::Image);

    assert_eq!(metadata.size, 4);

    assert!(!metadata.is_empty);
}
