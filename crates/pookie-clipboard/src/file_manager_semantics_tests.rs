use std::fs::File;
use std::io::Write;

use crate::file_image::canonicalize_single_local_file_uri;
use crate::wayland::mime::{ClipboardMimeKind, URI_LIST_MIME_TYPE, candidate_content_mimes};
use crate::{
    ClipboardContent, canonicalize_rgba, decode_canonical_png_to_rgba, preferred_image_mime,
};

///
/// Pure cross-platform decision simulator matching both X11 and Wayland semantics.
///
/// Invariant:
/// 1. Direct supported image MIME -> Image
/// 2. Else if text/uri-list is offered -> try canonicalize_single_local_file_uri:
///    - success -> Image
///    - failure for directory / PDF / non-image / multiple URIs / remote URI / malformed:
///      -> None (no ClipboardEvent, NO text fallback!)
/// 3. Else if genuine supported text MIME exists -> Text
///
fn evaluate_clipboard_event(
    offered_mimes: &[String],
    get_payload: impl Fn(&str) -> Option<Vec<u8>>,
) -> Option<ClipboardContent> {
    // 1. Direct supported image MIME -> Image
    if let Some(image_mime) = preferred_image_mime(offered_mimes)
        && let Some(payload) = get_payload(image_mime)
        && let Ok(canonical) = crate::image_codec::canonicalize_image(&payload, image_mime)
    {
        return Some(ClipboardContent::Image(canonical));
    }

    // 2. text/uri-list offered -> treat as file-copy operation
    if offered_mimes
        .iter()
        .any(|m| m.as_str() == URI_LIST_MIME_TYPE)
    {
        if let Some(payload) = get_payload(URI_LIST_MIME_TYPE)
            && let Ok(canonical) = canonicalize_single_local_file_uri(&payload)
        {
            return Some(ClipboardContent::Image(canonical));
        }
        // Failure for directory / PDF / non-image / multiple URIs / remote URI / malformed:
        // STOP. No ClipboardEvent. No text fallback!
        return None;
    }

    // 3. Genuine supported text MIME -> Text (only when text/uri-list was NOT offered)
    for preferred in crate::wayland::mime::SUPPORTED_TEXT_MIME_TYPES {
        if offered_mimes.iter().any(|m| m.as_str() == *preferred)
            && let Some(payload) = get_payload(preferred)
            && let Ok(text) = String::from_utf8(payload)
            && !text.is_empty()
        {
            return Some(ClipboardContent::Text(text));
        }
    }

    None
}

struct TestTempDir {
    path: std::path::PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "pookie_sem_{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_1_exactly_one_local_image_uri_yields_image() {
    let dir = TestTempDir::new("img");
    let image_path = dir.path.join("photo.png");
    let rgba = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let png = canonicalize_rgba(2, 2, &rgba).unwrap();
    let mut file = File::create(&image_path).unwrap();
    file.write_all(png.png_bytes()).unwrap();
    drop(file);

    let uri_payload = format!("file://{}\r\n", image_path.to_str().unwrap()).into_bytes();
    let text_payload = image_path.to_str().unwrap().as_bytes().to_vec();

    let offered = vec!["text/uri-list".to_string(), "text/plain".to_string()];
    let result = evaluate_clipboard_event(&offered, |mime| match mime {
        "text/uri-list" => Some(uri_payload.clone()),
        "text/plain" => Some(text_payload.clone()),
        _ => None,
    });

    let Some(ClipboardContent::Image(canonical)) = result else {
        panic!("expected image content from valid local image file URI");
    };

    let (width, height, _) = decode_canonical_png_to_rgba(canonical.png_bytes()).unwrap();
    assert_eq!(width, 2);
    assert_eq!(height, 2);
}

#[test]
fn test_2_pdf_uri_with_text_plain_offered_is_ignored() {
    let dir = TestTempDir::new("pdf");
    let pdf_path = dir.path.join("document.pdf");
    let mut file = File::create(&pdf_path).unwrap();
    file.write_all(b"%PDF-1.4 fake pdf data").unwrap();
    drop(file);

    let uri_payload = format!("file://{}\r\n", pdf_path.to_str().unwrap()).into_bytes();
    let text_payload = pdf_path.to_str().unwrap().as_bytes().to_vec();

    let offered = vec!["text/uri-list".to_string(), "text/plain".to_string()];
    let result = evaluate_clipboard_event(&offered, |mime| match mime {
        "text/uri-list" => Some(uri_payload.clone()),
        "text/plain" => Some(text_payload.clone()),
        _ => None,
    });

    assert_eq!(
        result, None,
        "PDF URI with text/plain must be ignored, NOT stored as Text"
    );

    let candidates = candidate_content_mimes(&offered);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].kind, ClipboardMimeKind::FileList);
}

#[test]
fn test_3_directory_uri_with_text_plain_offered_is_ignored() {
    let dir = TestTempDir::new("dir");
    let folder_path = dir.path.join("my_folder");
    std::fs::create_dir_all(&folder_path).unwrap();

    let uri_payload = format!("file://{}\r\n", folder_path.to_str().unwrap()).into_bytes();
    let text_payload = folder_path.to_str().unwrap().as_bytes().to_vec();

    let offered = vec!["text/uri-list".to_string(), "text/plain".to_string()];
    let result = evaluate_clipboard_event(&offered, |mime| match mime {
        "text/uri-list" => Some(uri_payload.clone()),
        "text/plain" => Some(text_payload.clone()),
        _ => None,
    });

    assert_eq!(
        result, None,
        "Directory URI with text/plain must be ignored, NOT stored as Text"
    );
}

#[test]
fn test_4_multiple_file_uris_with_text_plain_offered_is_ignored() {
    let dir = TestTempDir::new("multi");
    let img1 = dir.path.join("1.png");
    let img2 = dir.path.join("2.png");
    File::create(&img1).unwrap();
    File::create(&img2).unwrap();

    let uri_payload = format!(
        "file://{}\r\nfile://{}\r\n",
        img1.to_str().unwrap(),
        img2.to_str().unwrap()
    )
    .into_bytes();
    let text_payload =
        format!("{}\n{}", img1.to_str().unwrap(), img2.to_str().unwrap()).into_bytes();

    let offered = vec!["text/uri-list".to_string(), "text/plain".to_string()];
    let result = evaluate_clipboard_event(&offered, |mime| match mime {
        "text/uri-list" => Some(uri_payload.clone()),
        "text/plain" => Some(text_payload.clone()),
        _ => None,
    });

    assert_eq!(
        result, None,
        "Multiple file URIs with text/plain must be ignored, NOT stored as Text"
    );
}

#[test]
fn test_5_remote_uri_with_text_plain_offered_is_ignored() {
    let uri_payload = b"https://example.com/photo.png\r\n".to_vec();
    let text_payload = b"https://example.com/photo.png".to_vec();

    let offered = vec!["text/uri-list".to_string(), "text/plain".to_string()];
    let result = evaluate_clipboard_event(&offered, |mime| match mime {
        "text/uri-list" => Some(uri_payload.clone()),
        "text/plain" => Some(text_payload.clone()),
        _ => None,
    });

    assert_eq!(
        result, None,
        "Remote URI with text/plain must be ignored, NOT stored as Text"
    );
}

#[test]
fn test_6_malformed_uri_with_text_plain_offered_is_ignored() {
    let uri_payload = b"file://%ZZ/invalid_encoding\r\n".to_vec();
    let text_payload = b"file://%ZZ/invalid_encoding".to_vec();

    let offered = vec!["text/uri-list".to_string(), "text/plain".to_string()];
    let result = evaluate_clipboard_event(&offered, |mime| match mime {
        "text/uri-list" => Some(uri_payload.clone()),
        "text/plain" => Some(text_payload.clone()),
        _ => None,
    });

    assert_eq!(
        result, None,
        "Malformed URI with text/plain must be ignored, NOT stored as Text"
    );
}

#[test]
fn test_7_literal_file_path_copied_as_text_only_yields_text() {
    let text_path = "/home/user/file.pdf";
    let text_payload = text_path.as_bytes().to_vec();

    // Notice: only text MIME is offered, NO text/uri-list!
    let offered = vec![
        "text/plain;charset=utf-8".to_string(),
        "text/plain".to_string(),
    ];
    let result = evaluate_clipboard_event(&offered, |mime| {
        if mime == "text/plain;charset=utf-8" || mime == "text/plain" {
            Some(text_payload.clone())
        } else {
            None
        }
    });

    assert_eq!(
        result,
        Some(ClipboardContent::Text(text_path.to_string())),
        "literal file path copied from editor/terminal with text-only MIME must be captured as Text"
    );

    let candidates = candidate_content_mimes(&offered);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].kind, ClipboardMimeKind::Text);
}

#[test]
fn test_8_normal_arbitrary_text_yields_text() {
    let text_content = "Hello Pookie!\nThis is regular multi-line text.";
    let text_payload = text_content.as_bytes().to_vec();

    let offered = vec!["text/plain".to_string()];
    let result = evaluate_clipboard_event(&offered, |mime| {
        if mime == "text/plain" {
            Some(text_payload.clone())
        } else {
            None
        }
    });

    assert_eq!(
        result,
        Some(ClipboardContent::Text(text_content.to_string())),
        "normal arbitrary text must be captured as Text"
    );
}

#[test]
fn test_9_direct_image_mime_still_wins_over_uri_list() {
    let rgba = vec![
        10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
    ];
    let direct_png = canonicalize_rgba(2, 2, &rgba).unwrap();

    let dir = TestTempDir::new("direct_vs_uri");
    let fallback_path = dir.path.join("other.png");
    let fallback_rgba = vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
    let fallback_png = canonicalize_rgba(2, 2, &fallback_rgba).unwrap();
    let mut file = File::create(&fallback_path).unwrap();
    file.write_all(fallback_png.png_bytes()).unwrap();
    drop(file);

    let uri_payload = format!("file://{}\r\n", fallback_path.to_str().unwrap()).into_bytes();
    let text_payload = b"fallback text".to_vec();

    // App offers direct image/png, text/uri-list, and text/plain
    let offered = vec![
        "text/plain".to_string(),
        "text/uri-list".to_string(),
        "image/png".to_string(),
    ];

    let result = evaluate_clipboard_event(&offered, |mime| match mime {
        "image/png" => Some(direct_png.png_bytes().to_vec()),
        "text/uri-list" => Some(uri_payload.clone()),
        "text/plain" => Some(text_payload.clone()),
        _ => None,
    });

    let Some(ClipboardContent::Image(canonical)) = result else {
        panic!("expected direct image to win");
    };

    assert_eq!(
        canonical.identity(),
        direct_png.identity(),
        "direct image must win over URI-list"
    );

    let candidates = candidate_content_mimes(&offered);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].kind, ClipboardMimeKind::Image);
    assert_eq!(candidates[0].mime_type, "image/png");
}
