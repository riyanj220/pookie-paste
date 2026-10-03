use pookie_clipboard::ClipboardContent;
use pookie_core::ContentHasher;

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
fn text_hashing_remains_unversioned_sha256() {
    let content = ClipboardContent::Text("Hello Pookie".to_string());
    let hash = ContentHasher::hash(&content);

    // echo -n "Hello Pookie" | sha256sum
    assert_eq!(
        hash,
        "0a1cb1b27d789402f09642b01dcb64bbd864c45100da0921a744042d8c133f1e"
    );
    assert!(!hash.starts_with("rgba-v1:"));
}

#[test]
fn image_content_hasher_returns_versioned_rgba_v1_identity() {
    let canonical = pookie_clipboard::canonicalize_rgba(1, 1, &[255, 0, 0, 255]).unwrap();
    let expected_identity = canonical.identity().to_versioned_string();
    let image = ClipboardContent::Image(canonical);

    let hash = ContentHasher::hash(&image);

    assert_eq!(hash, expected_identity);
    assert!(hash.starts_with("rgba-v1:"));
}
