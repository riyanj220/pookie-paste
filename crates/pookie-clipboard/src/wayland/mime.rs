use crate::{is_supported_image_mime, preferred_image_mime};

pub const SUPPORTED_TEXT_MIME_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "text/plain",
];

pub const URI_LIST_MIME_TYPE: &str = "text/uri-list";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMimeKind {
    Text,

    Image,

    FileList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferredMime<'a> {
    pub mime_type: &'a str,

    pub kind: ClipboardMimeKind,
}

pub fn is_supported_text_mime(mime: &str) -> bool {
    SUPPORTED_TEXT_MIME_TYPES.contains(&mime)
}

pub fn is_supported_file_list_mime(mime: &str) -> bool {
    mime == URI_LIST_MIME_TYPE
}

pub fn is_supported_clipboard_mime(mime: &str) -> bool {
    is_supported_image_mime(mime)
        || is_supported_file_list_mime(mime)
        || is_supported_text_mime(mime)
}

///
/// Choose the best content representation offered by a
/// Wayland clipboard owner.
///
/// Priority:
/// 1. Direct image representation (image/png, etc.)
/// 2. Copied local file list (text/uri-list)
/// 3. Text fallbacks (text/plain, etc.)
///
#[allow(dead_code)]
pub fn preferred_content_mime(offered: &[String]) -> Option<PreferredMime<'_>> {
    candidate_content_mimes(offered).into_iter().next()
}

#[allow(dead_code)]
pub fn preferred_text_mime(offered: &[String]) -> Option<&str> {
    for preferred in SUPPORTED_TEXT_MIME_TYPES {
        if let Some(offered_mime) = offered.iter().find(|mime| mime.as_str() == *preferred) {
            return Some(offered_mime.as_str());
        }
    }

    None
}

pub fn candidate_content_mimes(offered: &[String]) -> Vec<PreferredMime<'_>> {
    if let Some(image_mime) = preferred_image_mime(offered) {
        return vec![PreferredMime {
            mime_type: image_mime,
            kind: ClipboardMimeKind::Image,
        }];
    }

    let mut candidates = Vec::new();

    if let Some(uri_list_mime) = offered
        .iter()
        .find(|mime| mime.as_str() == URI_LIST_MIME_TYPE)
    {
        candidates.push(PreferredMime {
            mime_type: uri_list_mime.as_str(),
            kind: ClipboardMimeKind::FileList,
        });
    }

    for preferred in SUPPORTED_TEXT_MIME_TYPES {
        if let Some(offered_mime) = offered.iter().find(|mime| mime.as_str() == *preferred)
            && !candidates
                .iter()
                .any(|c: &PreferredMime<'_>| c.mime_type == offered_mime.as_str())
        {
            candidates.push(PreferredMime {
                mime_type: offered_mime.as_str(),
                kind: ClipboardMimeKind::Text,
            });
        }
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardMimeKind, candidate_content_mimes, is_supported_clipboard_mime,
        is_supported_file_list_mime, preferred_content_mime, preferred_text_mime,
    };

    #[test]
    fn recognizes_supported_text_mime() {
        assert!(is_supported_clipboard_mime("text/plain;charset=utf-8",));

        assert!(is_supported_clipboard_mime("text/plain",));
    }

    #[test]
    fn recognizes_supported_image_mime() {
        assert!(is_supported_clipboard_mime("image/png",));

        assert!(is_supported_clipboard_mime("image/jpeg",));

        assert!(is_supported_clipboard_mime("image/webp",));
    }

    #[test]
    fn rejects_unrelated_mime() {
        assert!(!is_supported_clipboard_mime("text/html",));

        assert!(!is_supported_clipboard_mime("application/pdf",));
    }

    #[test]
    fn image_has_priority_over_text() {
        let offered = vec![
            "text/plain;charset=utf-8".to_string(),
            "image/jpeg".to_string(),
            "image/png".to_string(),
        ];

        let preferred = preferred_content_mime(&offered).expect("preferred MIME missing");

        assert_eq!(preferred.kind, ClipboardMimeKind::Image,);

        assert_eq!(preferred.mime_type, "image/png",);
    }

    #[test]
    fn falls_back_to_text() {
        let offered = vec!["text/plain".to_string()];

        let preferred = preferred_content_mime(&offered).expect("preferred MIME missing");

        assert_eq!(preferred.kind, ClipboardMimeKind::Text,);

        assert_eq!(preferred.mime_type, "text/plain",);
    }

    #[test]
    fn preferred_text_preserves_existing_priority() {
        let offered = vec![
            "text/plain".to_string(),
            "text/plain;charset=utf-8".to_string(),
        ];

        assert_eq!(
            preferred_text_mime(&offered,),
            Some("text/plain;charset=utf-8"),
        );
    }

    #[test]
    fn candidate_mimes_preserves_text_priority_order() {
        let offered = vec![
            "text/plain".to_string(),
            "text/plain;charset=utf-8".to_string(),
        ];

        let candidates = candidate_content_mimes(&offered);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].mime_type, "text/plain;charset=utf-8");
        assert_eq!(candidates[0].kind, ClipboardMimeKind::Text);
        assert_eq!(candidates[1].mime_type, "text/plain");
        assert_eq!(candidates[1].kind, ClipboardMimeKind::Text);
    }

    #[test]
    fn candidate_mimes_image_does_not_include_text_fallbacks() {
        let offered = vec![
            "text/plain;charset=utf-8".to_string(),
            "image/png".to_string(),
            "text/plain".to_string(),
        ];

        let candidates = candidate_content_mimes(&offered);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].mime_type, "image/png");
        assert_eq!(candidates[0].kind, ClipboardMimeKind::Image);
    }

    #[test]
    fn candidate_mimes_empty_when_no_supported_mimes() {
        let offered = vec!["application/pdf".to_string(), "text/html".to_string()];

        let candidates = candidate_content_mimes(&offered);
        assert!(candidates.is_empty());
    }

    #[test]
    fn recognizes_supported_file_list_mime() {
        assert!(is_supported_clipboard_mime("text/uri-list"));
        assert!(is_supported_file_list_mime("text/uri-list"));
    }

    #[test]
    fn direct_image_wins_over_uri_list_and_text() {
        let offered = vec![
            "text/plain;charset=utf-8".to_string(),
            "text/uri-list".to_string(),
            "image/png".to_string(),
        ];

        let candidates = candidate_content_mimes(&offered);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].mime_type, "image/png");
        assert_eq!(candidates[0].kind, ClipboardMimeKind::Image);
    }

    #[test]
    fn uri_list_takes_priority_over_text_and_preserves_text_fallback() {
        let offered = vec!["text/plain".to_string(), "text/uri-list".to_string()];

        let candidates = candidate_content_mimes(&offered);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].mime_type, "text/uri-list");
        assert_eq!(candidates[0].kind, ClipboardMimeKind::FileList);
        assert_eq!(candidates[1].mime_type, "text/plain");
        assert_eq!(candidates[1].kind, ClipboardMimeKind::Text);
    }

    #[test]
    fn uri_list_only_yields_file_list_candidate() {
        let offered = vec!["text/uri-list".to_string()];

        let candidates = candidate_content_mimes(&offered);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].mime_type, "text/uri-list");
        assert_eq!(candidates[0].kind, ClipboardMimeKind::FileList);
    }

    #[test]
    fn text_plain_only_yields_text_candidate() {
        let offered = vec!["text/plain".to_string()];

        let candidates = candidate_content_mimes(&offered);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].mime_type, "text/plain");
        assert_eq!(candidates[0].kind, ClipboardMimeKind::Text);
    }
}
