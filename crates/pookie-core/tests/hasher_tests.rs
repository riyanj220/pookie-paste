use pookie_core::ContentHasher;

use pookie_clipboard::ClipboardContent;

#[test]
fn generates_same_hash_for_same_text() {
    let first = ClipboardContent::Text("Hello Pookie".to_string());

    let second = ClipboardContent::Text("Hello Pookie".to_string());

    let first_hash = ContentHasher::hash(&first);

    let second_hash = ContentHasher::hash(&second);

    assert_eq!(first_hash, second_hash);
}

#[test]
fn generates_different_hash_for_different_text() {
    let first = ClipboardContent::Text("Hello".to_string());

    let second = ClipboardContent::Text("World".to_string());

    let first_hash = ContentHasher::hash(&first);

    let second_hash = ContentHasher::hash(&second);

    assert_ne!(first_hash, second_hash);
}

#[test]
fn generates_hash_for_images() {
    let image = ClipboardContent::Image(vec![1, 2, 3, 4]);

    let hash = ContentHasher::hash(&image);

    assert!(!hash.is_empty());
}
