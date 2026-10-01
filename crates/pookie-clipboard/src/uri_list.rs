use std::fmt;
use std::path::PathBuf;

///
/// Errors that can occur when parsing a `text/uri-list` payload.
///
/// Error variants avoid retaining arbitrary input strings from the
/// clipboard to prevent memory bloat or unintended payload leaks.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UriListError {
    /// Clipboard payload was empty or contained only comments/whitespace.
    EmptyPayload,

    /// Multiple URIs were present in the payload.
    MultipleUris { count: usize },

    /// URI scheme is not `file:`.
    UnsupportedScheme,

    /// URI specifies a remote or non-localhost authority.
    RemoteHost,

    /// URI contains a malformed percent-escape sequence (e.g. `%`, `%2`, `%2G`).
    MalformedPercentEncoding,

    /// URI path is empty.
    EmptyPath,

    /// URI contains unencoded `?` (query) or `#` (fragment) delimiters.
    ContainsQueryOrFragment,

    /// URI path cannot be interpreted as a valid absolute filesystem path.
    InvalidPath,
}

impl fmt::Display for UriListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPayload => {
                write!(formatter, "URI list payload is empty")
            }

            Self::MultipleUris { count } => {
                write!(
                    formatter,
                    "URI list contains {count} entries; exactly one is required"
                )
            }

            Self::UnsupportedScheme => {
                write!(formatter, "URI scheme is not file://")
            }

            Self::RemoteHost => {
                write!(formatter, "URI references a remote authority")
            }

            Self::MalformedPercentEncoding => {
                write!(formatter, "URI contains malformed percent encoding")
            }

            Self::EmptyPath => {
                write!(formatter, "URI path is empty")
            }

            Self::ContainsQueryOrFragment => {
                write!(
                    formatter,
                    "URI contains unencoded query or fragment delimiter"
                )
            }

            Self::InvalidPath => {
                write!(formatter, "URI does not resolve to a valid local path")
            }
        }
    }
}

impl std::error::Error for UriListError {}

///
/// Parse a single local file URI from a `text/uri-list` byte payload.
///
/// Complies with RFC 2483 / RFC 8089 `text/uri-list` semantics:
/// - Lines delimited by CRLF or LF.
/// - Comment lines beginning with `#` are ignored.
/// - Blank lines are ignored.
/// - Must contain exactly one non-comment URI entry.
/// - Scheme must be `file:` (case-insensitive).
/// - Authority must be empty (`file:///path`) or `localhost` (`file://localhost/path`).
/// - Query (`?`) and fragment (`#`) delimiters are rejected.
/// - Decoded bytes are converted directly to a Unix `PathBuf` without lossy UTF-8 conversion.
///
pub(crate) fn parse_single_local_file_uri(payload: &[u8]) -> Result<PathBuf, UriListError> {
    if payload.is_empty() {
        return Err(UriListError::EmptyPayload);
    }

    let mut uri_lines = Vec::new();

    for line in payload.split(|&b| b == b'\n') {
        let trimmed = trim_ascii_whitespace(line);

        if trimmed.is_empty() || trimmed.starts_with(b"#") {
            continue;
        }

        uri_lines.push(trimmed);
    }

    match uri_lines.len() {
        0 => Err(UriListError::EmptyPayload),
        1 => parse_single_uri(uri_lines[0]),
        count => Err(UriListError::MultipleUris { count }),
    }
}

///
/// Convenience wrapper around `parse_single_local_file_uri` for string slices.
///
#[cfg(test)]
pub(crate) fn parse_single_local_file_uri_str(payload: &str) -> Result<PathBuf, UriListError> {
    parse_single_local_file_uri(payload.as_bytes())
}

fn trim_ascii_whitespace(slice: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < slice.len() && (slice[start].is_ascii_whitespace() || slice[start] == b'\r') {
        start += 1;
    }

    let mut end = slice.len();
    while end > start && (slice[end - 1].is_ascii_whitespace() || slice[end - 1] == b'\r') {
        end -= 1;
    }

    &slice[start..end]
}

fn parse_single_uri(uri: &[u8]) -> Result<PathBuf, UriListError> {
    if uri.len() < 5 || !uri[..5].eq_ignore_ascii_case(b"file:") {
        return Err(UriListError::UnsupportedScheme);
    }

    let rest = &uri[5..];

    let raw_path: &[u8] = if rest.starts_with(b"//") {
        let after_slashes = &rest[2..];

        if after_slashes.starts_with(b"/") {
            // file:///path
            // Reject file:////path or more slashes as ambiguous/remote
            if after_slashes.starts_with(b"//") {
                return Err(UriListError::RemoteHost);
            }
            after_slashes
        } else if let Some(slash_idx) = after_slashes.iter().position(|&b| b == b'/') {
            let authority = &after_slashes[..slash_idx];

            if authority.eq_ignore_ascii_case(b"localhost") {
                &after_slashes[slash_idx..]
            } else {
                return Err(UriListError::RemoteHost);
            }
        } else {
            return Err(UriListError::EmptyPath);
        }
    } else if rest.starts_with(b"/") {
        // file:/path (RFC 8089 allows file: with no authority)
        rest
    } else {
        return Err(UriListError::InvalidPath);
    };

    if raw_path.is_empty() {
        return Err(UriListError::EmptyPath);
    }

    // Literal '?' or '#' indicates query or fragment
    if raw_path.contains(&b'?') || raw_path.contains(&b'#') {
        return Err(UriListError::ContainsQueryOrFragment);
    }

    let decoded = decode_percent(raw_path)?;

    if decoded.is_empty() {
        return Err(UriListError::EmptyPath);
    }

    if decoded.contains(&0) {
        return Err(UriListError::InvalidPath);
    }

    if !decoded.starts_with(b"/") {
        return Err(UriListError::InvalidPath);
    }

    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        Ok(PathBuf::from(OsStr::from_bytes(&decoded)))
    }

    #[cfg(not(unix))]
    {
        let string = std::str::from_utf8(&decoded).map_err(|_| UriListError::InvalidPath)?;
        Ok(PathBuf::from(string))
    }
}

fn decode_percent(raw_path: &[u8]) -> Result<Vec<u8>, UriListError> {
    let mut decoded = Vec::with_capacity(raw_path.len());
    let mut index = 0;

    while index < raw_path.len() {
        if raw_path[index] == b'%' {
            if index + 2 >= raw_path.len() {
                return Err(UriListError::MalformedPercentEncoding);
            }

            let high = from_hex_digit(raw_path[index + 1])
                .ok_or(UriListError::MalformedPercentEncoding)?;
            let low = from_hex_digit(raw_path[index + 2])
                .ok_or(UriListError::MalformedPercentEncoding)?;

            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(raw_path[index]);
            index += 1;
        }
    }

    Ok(decoded)
}

fn from_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{UriListError, parse_single_local_file_uri, parse_single_local_file_uri_str};

    #[test]
    fn parses_standard_file_uri() {
        let uri = "file:///home/user/image.png\r\n";
        let path = parse_single_local_file_uri_str(uri).unwrap();
        assert_eq!(path, PathBuf::from("/home/user/image.png"));
    }

    #[test]
    fn parses_localhost_authority_uri() {
        let uri = "file://localhost/home/user/image.png\n";
        let path = parse_single_local_file_uri_str(uri).unwrap();
        assert_eq!(path, PathBuf::from("/home/user/image.png"));
    }

    #[test]
    fn parses_case_insensitive_localhost_and_scheme() {
        let uri = "FILE://LOCALHOST/tmp/test.jpg";
        let path = parse_single_local_file_uri_str(uri).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/test.jpg"));
    }

    #[test]
    fn parses_percent_encoded_spaces() {
        let uri = "file:///home/user/My%20Screenshots/pic%201.png";
        let path = parse_single_local_file_uri_str(uri).unwrap();
        assert_eq!(path, PathBuf::from("/home/user/My Screenshots/pic 1.png"));
    }

    #[test]
    fn parses_percent_encoded_special_characters() {
        // %23 is '#', %3F is '?', %25 is '%'
        let uri = "file:///tmp/file%23name%3Fwith%25percent.png";
        let path = parse_single_local_file_uri_str(uri).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/file#name?with%percent.png"));
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let payload =
            "# This is a comment\r\n\r\n# Another comment\nfile:///home/user/photo.jpg\r\n\n";
        let path = parse_single_local_file_uri_str(payload).unwrap();
        assert_eq!(path, PathBuf::from("/home/user/photo.jpg"));
    }

    #[test]
    fn distinguishes_comment_from_percent_encoded_hash_file() {
        let payload = "# comment line\nfile:///tmp/has%23hash.png";
        let path = parse_single_local_file_uri_str(payload).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/has#hash.png"));
    }

    #[test]
    fn rejects_unencoded_query_or_fragment() {
        assert_eq!(
            parse_single_local_file_uri_str("file:///tmp/image.png?size=large"),
            Err(UriListError::ContainsQueryOrFragment)
        );

        assert_eq!(
            parse_single_local_file_uri_str("file:///tmp/image.png#section"),
            Err(UriListError::ContainsQueryOrFragment)
        );
    }

    #[test]
    fn rejects_malformed_percent_encoding() {
        assert_eq!(
            parse_single_local_file_uri_str("file:///tmp/image%2.png"),
            Err(UriListError::MalformedPercentEncoding)
        );

        assert_eq!(
            parse_single_local_file_uri_str("file:///tmp/image%2G.png"),
            Err(UriListError::MalformedPercentEncoding)
        );

        assert_eq!(
            parse_single_local_file_uri_str("file:///tmp/image%"),
            Err(UriListError::MalformedPercentEncoding)
        );

        assert_eq!(
            parse_single_local_file_uri_str("file:///tmp/image%%20.png"),
            Err(UriListError::MalformedPercentEncoding)
        );
    }

    #[test]
    fn rejects_remote_host() {
        assert_eq!(
            parse_single_local_file_uri_str("file://example.com/data/img.png"),
            Err(UriListError::RemoteHost)
        );

        assert_eq!(
            parse_single_local_file_uri_str("file://192.168.1.1/data/img.png"),
            Err(UriListError::RemoteHost)
        );

        assert_eq!(
            parse_single_local_file_uri_str("file:////remote/share"),
            Err(UriListError::RemoteHost)
        );
    }

    #[test]
    fn rejects_unsupported_schemes() {
        assert_eq!(
            parse_single_local_file_uri_str("http://example.com/img.png"),
            Err(UriListError::UnsupportedScheme)
        );

        assert_eq!(
            parse_single_local_file_uri_str("https://example.com/img.png"),
            Err(UriListError::UnsupportedScheme)
        );

        assert_eq!(
            parse_single_local_file_uri_str("data:image/png;base64,AAAA"),
            Err(UriListError::UnsupportedScheme)
        );
    }

    #[test]
    fn rejects_empty_payload() {
        assert_eq!(
            parse_single_local_file_uri_str(""),
            Err(UriListError::EmptyPayload)
        );

        assert_eq!(
            parse_single_local_file_uri_str("   \r\n  \n"),
            Err(UriListError::EmptyPayload)
        );

        assert_eq!(
            parse_single_local_file_uri_str("# comment 1\r\n# comment 2\n"),
            Err(UriListError::EmptyPayload)
        );
    }

    #[test]
    fn rejects_multiple_uris() {
        let payload = "file:///home/user/1.png\r\nfile:///home/user/2.png\r\n";
        assert_eq!(
            parse_single_local_file_uri_str(payload),
            Err(UriListError::MultipleUris { count: 2 })
        );
    }

    #[test]
    fn does_not_silently_parse_gnome_action_lines() {
        // copy or cut followed by file should be treated strictly: 2 lines
        let payload = "copy\nfile:///home/user/img.png\n";
        assert_eq!(
            parse_single_local_file_uri_str(payload),
            Err(UriListError::MultipleUris { count: 2 })
        );

        let single_action = "copy\n";
        assert_eq!(
            parse_single_local_file_uri_str(single_action),
            Err(UriListError::UnsupportedScheme)
        );
    }

    #[test]
    #[cfg(unix)]
    fn preserves_non_utf8_unix_path_bytes() {
        use std::os::unix::ffi::OsStrExt;

        // Construct a URI with non-UTF-8 bytes (0xFF and 0xFE) in filename
        let payload = b"file:///tmp/image_\xFF\xFE.png";
        let path = parse_single_local_file_uri(payload).unwrap();
        assert_eq!(path.as_os_str().as_bytes(), b"/tmp/image_\xFF\xFE.png");

        // Percent-encoded non-UTF-8 bytes
        let encoded_payload = b"file:///tmp/image_%FF%FE.png";
        let encoded_path = parse_single_local_file_uri(encoded_payload).unwrap();
        assert_eq!(
            encoded_path.as_os_str().as_bytes(),
            b"/tmp/image_\xFF\xFE.png"
        );
    }

    #[test]
    fn rejects_nul_byte_in_path() {
        assert_eq!(
            parse_single_local_file_uri_str("file:///tmp/img%00.png"),
            Err(UriListError::InvalidPath)
        );
    }
}
