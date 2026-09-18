use crate::{is_supported_image_mime, preferred_image_mime};

pub const SUPPORTED_TEXT_MIME_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "text/plain",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMimeKind {
    Text,

    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferredMime<'a> {
    pub mime_type: &'a str,

    pub kind: ClipboardMimeKind,
}

pub fn is_supported_text_mime(mime: &str) -> bool {
    SUPPORTED_TEXT_MIME_TYPES.contains(&mime)
}

pub fn is_supported_clipboard_mime(mime: &str) -> bool {
    is_supported_image_mime(mime) || is_supported_text_mime(mime)
}

///
/// Choose the best content representation offered by a
/// Wayland clipboard owner.
///
/// Image representations intentionally take priority over
/// text fallbacks.
///
/// Browsers and image applications commonly expose both an
/// image representation and textual fallback data for one
/// copied image. Pookie should capture that as an image.
///
pub fn preferred_content_mime(offered: &[String]) -> Option<PreferredMime<'_>> {
    if let Some(image_mime) = preferred_image_mime(offered) {
        return Some(PreferredMime {
            mime_type: image_mime,

            kind: ClipboardMimeKind::Image,
        });
    }

    preferred_text_mime(offered).map(|text_mime| PreferredMime {
        mime_type: text_mime,

        kind: ClipboardMimeKind::Text,
    })
}

pub fn preferred_text_mime(offered: &[String]) -> Option<&str> {
    for preferred in SUPPORTED_TEXT_MIME_TYPES {
        if let Some(offered_mime) = offered.iter().find(|mime| mime.as_str() == *preferred) {
            return Some(offered_mime.as_str());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardMimeKind, is_supported_clipboard_mime, preferred_content_mime, preferred_text_mime,
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
}
