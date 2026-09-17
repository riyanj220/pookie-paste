use std::{fs::File, io::Read, os::fd::OwnedFd};

use tokio::sync::mpsc::Sender;

use crate::{ClipboardContent, ClipboardEvent, canonicalize_image};

use super::mime::ClipboardMimeKind;

/*
 * This is a transfer-safety ceiling, not Pookie's history
 * policy.
 *
 * The canonical ClipboardPolicy remains responsible for the
 * normal 1 MiB text / 10 MiB image limits after processing.
 *
 * This higher ceiling merely prevents an untrusted clipboard
 * owner from making Pookie allocate an unbounded buffer while
 * receiving data from a Wayland pipe.
 */
const MAX_CLIPBOARD_TRANSFER_BYTES: u64 = 64 * 1024 * 1024;

pub fn read_clipboard_fd(
    fd: OwnedFd,
    mime_type: &str,
    kind: ClipboardMimeKind,
) -> Result<ClipboardContent, String> {
    let mut file = File::from(fd);

    let bytes = read_limited(&mut file)?;

    decode_clipboard_payload(bytes, mime_type, kind)
}

fn read_limited<R>(reader: &mut R) -> Result<Vec<u8>, String>
where
    R: Read,
{
    /*
     * Read one byte past the limit so we can distinguish
     * exactly-at-limit from oversized data.
     */
    let mut limited = reader.take(MAX_CLIPBOARD_TRANSFER_BYTES + 1);

    let mut contents = Vec::new();

    limited
        .read_to_end(&mut contents)
        .map_err(|error| format!("failed reading clipboard fd: {error}"))?;

    if contents.len() as u64 > MAX_CLIPBOARD_TRANSFER_BYTES {
        return Err(format!(
            "clipboard payload exceeded transfer safety limit of {} bytes",
            MAX_CLIPBOARD_TRANSFER_BYTES,
        ));
    }

    Ok(contents)
}

fn decode_clipboard_payload(
    bytes: Vec<u8>,
    mime_type: &str,
    kind: ClipboardMimeKind,
) -> Result<ClipboardContent, String> {
    if bytes.is_empty() {
        return Err("clipboard payload is empty".to_string());
    }

    match kind {
        ClipboardMimeKind::Text => {
            let text = String::from_utf8(bytes)
                .map_err(|error| format!("clipboard text is not valid UTF-8: {error}"))?;

            Ok(ClipboardContent::Text(text))
        }

        ClipboardMimeKind::Image => {
            let canonical = canonicalize_image(&bytes, mime_type).map_err(|error| {
                format!("failed canonicalizing Wayland clipboard image: {error}")
            })?;

            Ok(ClipboardContent::Image(canonical))
        }
    }
}

pub fn send_clipboard_event(sender: Sender<ClipboardEvent>, content: ClipboardContent) {
    if content_is_empty(&content) {
        tracing::debug!("ignoring empty clipboard payload");

        return;
    }

    let event = ClipboardEvent::new(content);

    if let Err(error) = sender.blocking_send(event) {
        tracing::error!(
            error = %error,
            "failed sending clipboard event"
        );
    }
}

fn content_is_empty(content: &ClipboardContent) -> bool {
    match content {
        ClipboardContent::Text(text) => text.is_empty(),

        ClipboardContent::Image(image) => image.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use crate::{ClipboardContent, canonicalize_rgba, decode_canonical_png_to_rgba};

    use super::{ClipboardMimeKind, content_is_empty, decode_clipboard_payload};

    #[test]
    fn decodes_text_payload() {
        let content =
            decode_clipboard_payload(b"hello".to_vec(), "text/plain", ClipboardMimeKind::Text)
                .expect("text decode failed");

        assert_eq!(content, ClipboardContent::Text("hello".to_string(),),);
    }

    #[test]
    fn rejects_invalid_utf8_text() {
        let result =
            decode_clipboard_payload(vec![0xff, 0xfe], "text/plain", ClipboardMimeKind::Text);

        assert!(result.is_err());
    }

    #[test]
    fn decodes_and_canonicalizes_image_payload() {
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];

        let png = canonicalize_rgba(2, 2, &rgba).expect("failed creating test PNG");

        let content = decode_clipboard_payload(png, "image/png", ClipboardMimeKind::Image)
            .expect("image decode failed");

        let ClipboardContent::Image(canonical) = content else {
            panic!("expected image content");
        };

        let (width, height, decoded) =
            decode_canonical_png_to_rgba(&canonical).expect("canonical PNG decode failed");

        assert_eq!(width, 2);

        assert_eq!(height, 2);

        assert_eq!(decoded, rgba);
    }

    #[test]
    fn rejects_empty_payload() {
        let result = decode_clipboard_payload(Vec::new(), "text/plain", ClipboardMimeKind::Text);

        assert!(result.is_err());
    }

    #[test]
    fn detects_empty_content() {
        assert!(content_is_empty(&ClipboardContent::Text(String::new(),),));

        assert!(content_is_empty(&ClipboardContent::Image(Vec::new(),),));
    }
}
