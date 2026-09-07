pub const SUPPORTED_MIME_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "text/plain",
];

pub fn is_supported_text_mime(mime: &str) -> bool {
    SUPPORTED_MIME_TYPES.contains(&mime)
}

pub fn preferred_text_mime(offered: &[String]) -> Option<&str> {
    let priorities = [
        "text/plain;charset=utf-8",
        "text/plain;charset=UTF-8",
        "text/plain",
    ];

    for preferred in priorities {
        if offered.iter().any(|mime| mime == preferred) {
            return Some(preferred);
        }
    }

    None
}
