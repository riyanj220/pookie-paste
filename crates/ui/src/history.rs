use ipc::HistoryContentRef;

const MAX_PREVIEW_LINES: usize = 3;
const MAX_CHARS_PER_LINE: usize = 70;

pub(crate) enum HistoryState {
    Loading,
    Loaded(Vec<ipc::HistoryItem>),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryRowKind<'a> {
    Text(&'a str),
    Image { file_path: &'a str },
    Invalid,
}

pub(crate) fn history_row_kind(item: &ipc::HistoryItem) -> HistoryRowKind<'_> {
    match item.content() {
        Ok(HistoryContentRef::Text(text)) => HistoryRowKind::Text(text),

        Ok(HistoryContentRef::Image { file_path }) => HistoryRowKind::Image { file_path },

        Err(error) => {
            tracing::debug!(
                item_id = %item.id,
                error = %error,
                "invalid history item received by UI"
            );

            HistoryRowKind::Invalid
        }
    }
}

pub(crate) fn preview_text(text: &str) -> String {
    let mut preview = String::new();

    let mut truncated = false;

    let mut lines = text.lines().peekable();

    for line_index in 0..MAX_PREVIEW_LINES {
        let Some(line) = lines.next() else {
            break;
        };

        if line_index > 0 {
            preview.push('\n');
        }

        let mut chars = line.chars();

        for _ in 0..MAX_CHARS_PER_LINE {
            let Some(ch) = chars.next() else {
                break;
            };

            preview.push(ch);
        }

        if chars.next().is_some() {
            truncated = true;
        }
    }

    if lines.next().is_some() {
        truncated = true;
    }

    if truncated {
        preview.push('…');
    }

    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_item(text: &str) -> ipc::HistoryItem {
        ipc::HistoryItem::text(
            "text-item".to_string(),
            text.to_string(),
            "2026-09-17T10:00:00Z".to_string(),
        )
    }

    fn image_item() -> ipc::HistoryItem {
        ipc::HistoryItem::image(
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "images/550e8400-e29b-41d4-a716-446655440000.png".to_string(),
            "2026-09-17T10:00:00Z".to_string(),
        )
        .expect("failed creating image history item")
    }

    #[test]
    fn short_preview_is_unchanged() {
        assert_eq!(preview_text("Hello world"), "Hello world");
    }

    #[test]
    fn preview_limits_number_of_lines() {
        let preview = preview_text("one\ntwo\nthree\nfour");

        assert_eq!(preview, "one\ntwo\nthree…");
    }

    #[test]
    fn preview_limits_long_lines() {
        let input = "a".repeat(MAX_CHARS_PER_LINE + 20);

        let preview = preview_text(&input);

        assert!(preview.ends_with('…'));
    }

    #[test]
    fn text_item_maps_to_text_row() {
        let item = text_item("hello");

        assert_eq!(history_row_kind(&item), HistoryRowKind::Text("hello"));
    }

    #[test]
    fn image_item_maps_to_image_row_with_path() {
        let item = image_item();

        assert_eq!(
            history_row_kind(&item),
            HistoryRowKind::Image {
                file_path: "images/550e8400-e29b-41d4-a716-446655440000.png",
            },
        );
    }

    #[test]
    fn malformed_item_still_maps_to_visible_row_kind() {
        let item = ipc::HistoryItem {
            id: "broken".to_string(),

            content_type: "image".to_string(),

            text_content: None,

            file_path: None,

            created_at: "2026-09-17T10:00:00Z".to_string(),

            pinned_at: None,
        };

        assert_eq!(history_row_kind(&item), HistoryRowKind::Invalid);
    }
}
