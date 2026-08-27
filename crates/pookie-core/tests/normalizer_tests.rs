use pookie_core::ContentNormalizer;

use pookie_clipboard::ClipboardContent;

#[test]
fn trims_text_whitespace() {
    let content = ClipboardContent::Text("   Hello Pookie   ".to_string());

    let normalized = ContentNormalizer::normalize(content);

    match normalized {
        ClipboardContent::Text(value) => {
            assert_eq!(value, "Hello Pookie");
        }

        _ => panic!("Expected text"),
    }
}

#[test]
fn normalizes_line_endings() {
    let content = ClipboardContent::Text("Hello\r\nWorld".to_string());

    let normalized = ContentNormalizer::normalize(content);

    match normalized {
        ClipboardContent::Text(value) => {
            assert_eq!(value, "Hello\nWorld");
        }

        _ => panic!("Expected text"),
    }
}

#[test]
fn keeps_images_unchanged() {
    let content = ClipboardContent::Image(vec![1, 2, 3]);

    let normalized = ContentNormalizer::normalize(content);

    match normalized {
        ClipboardContent::Image(value) => {
            assert_eq!(value, vec![1, 2, 3]);
        }

        _ => panic!("Expected image"),
    }
}

#[test]
fn trims_whitespace_only_text_to_empty() {
    let content = ClipboardContent::Text("      ".to_string());

    let normalized = ContentNormalizer::normalize(content);

    match normalized {
        ClipboardContent::Text(value) => {
            assert_eq!(value, "");
        }

        _ => panic!("Expected text"),
    }
}

#[test]
fn normalizes_mixed_line_endings() {
    let content = ClipboardContent::Text("Hello\rWorld\r\nRust".to_string());

    let normalized = ContentNormalizer::normalize(content);

    match normalized {
        ClipboardContent::Text(value) => {
            assert_eq!(value, "Hello\nWorld\nRust");
        }

        _ => panic!("Expected text"),
    }
}

#[test]
fn preserves_internal_spaces() {
    let content = ClipboardContent::Text("Hello     World".to_string());

    let normalized = ContentNormalizer::normalize(content);

    match normalized {
        ClipboardContent::Text(value) => {
            assert_eq!(value, "Hello     World");
        }

        _ => panic!("Expected text"),
    }
}
