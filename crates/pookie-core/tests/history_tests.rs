use pookie_core::ClipboardHistory;

#[test]
fn tracks_hashes() {
    let mut history = ClipboardHistory::default();

    assert!(!history.contains("abc123"));

    history.insert("abc123".to_string());

    assert!(history.contains("abc123"));
}

#[test]
fn detects_duplicate_hashes() {
    let mut history = ClipboardHistory::default();

    let first = history.check_and_insert("abc123".to_string());

    assert!(first);

    let second = history.check_and_insert("abc123".to_string());

    assert!(!second);
}
