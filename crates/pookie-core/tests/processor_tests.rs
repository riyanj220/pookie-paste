use chrono::Utc;

use pookie_core::{ClipboardEvent, ClipboardPolicy, ClipboardProcessor};

use pookie_clipboard::ClipboardContent;

#[test]
fn processes_valid_clipboard_event() {
    let processor = ClipboardProcessor::new();

    let event = ClipboardEvent {
        content: ClipboardContent::Text("Hello".to_string()),
        created_at: Utc::now(),
    };

    let item = processor.process(event).expect("Expected clipboard item");

    assert!(!item.hash.is_empty());
}

#[test]
fn ignores_empty_clipboard_event() {
    let processor = ClipboardProcessor::new();

    let event = ClipboardEvent {
        content: ClipboardContent::Text(String::new()),
        created_at: Utc::now(),
    };

    let item = processor.process(event);

    assert!(item.is_none());
}

#[test]
fn normalizes_content_before_creating_item() {
    let processor = ClipboardProcessor::new();

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

#[test]
fn processes_repeated_valid_clipboard_content() {
    let processor = ClipboardProcessor::new();

    let first_event = ClipboardEvent {
        content: ClipboardContent::Text("Hello Pookie".to_string()),
        created_at: Utc::now(),
    };

    let second_event = ClipboardEvent {
        content: ClipboardContent::Text("Hello Pookie".to_string()),
        created_at: Utc::now(),
    };

    let first = processor
        .process(first_event)
        .expect("first item should be processed");

    let second = processor
        .process(second_event)
        .expect("second item should also be processed");

    assert_eq!(first.hash, second.hash);
}

#[test]
fn accepts_different_clipboard_content() {
    let processor = ClipboardProcessor::new();

    let first_event = ClipboardEvent {
        content: ClipboardContent::Text("Hello".to_string()),
        created_at: Utc::now(),
    };

    let second_event = ClipboardEvent {
        content: ClipboardContent::Text("World".to_string()),
        created_at: Utc::now(),
    };

    let first = processor.process(first_event);

    let second = processor.process(second_event);

    assert!(first.is_some());
    assert!(second.is_some());
}

#[test]
fn rejects_content_by_policy() {
    let processor = ClipboardProcessor::new();

    let event = ClipboardEvent {
        content: ClipboardContent::Text("a".repeat(ClipboardPolicy::MAX_TEXT_SIZE + 1)),
        created_at: Utc::now(),
    };

    let result = processor.process(event);

    assert!(result.is_none());
}
