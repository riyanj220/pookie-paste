use chrono::Utc;

use pookie_core::{ClipboardEvent, ClipboardProcessor};

use pookie_clipboard::ClipboardContent;

#[test]
fn processes_valid_clipboard_event() {
    let processor = ClipboardProcessor;

    let event = ClipboardEvent {
        content: ClipboardContent::Text("Hello".to_string()),

        created_at: Utc::now(),
    };

    let item = processor.process(event).expect("Expected clipboard item");

    assert!(!item.hash.is_empty());
}

#[test]
fn ignores_empty_clipboard_event() {
    let processor = ClipboardProcessor;

    let event = ClipboardEvent {
        content: ClipboardContent::Text(String::new()),

        created_at: Utc::now(),
    };

    let item = processor.process(event);

    assert!(item.is_none());
}

#[test]
fn normalizes_content_before_creating_item() {
    let processor = ClipboardProcessor;

    let event = ClipboardEvent {
        content: ClipboardContent::Text("   Hello   ".to_string()),

        created_at: Utc::now(),
    };

    let item = processor.process(event).expect("Expected clipboard item");

    match item.content {
        ClipboardContent::Text(value) => {
            assert_eq!(value, "Hello");
        }

        _ => panic!("Expected text"),
    }
}
