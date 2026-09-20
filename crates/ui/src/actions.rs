use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveMenu {
    pub(crate) item_id: String,
    pub(crate) is_pinned: bool,
    pub(crate) button_rect: egui::Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuActionKind {
    Pin,
    Unpin,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiActionOutcome {
    PinToggled { id: String, is_pinned: bool },
    Deleted { id: String },
    Cleared { count: u64 },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeaderResponse {
    pub(crate) close_clicked: bool,
    pub(crate) clear_clicked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpinned_item_shows_pin_label() {
        let is_pinned = false;

        let pin_label = if is_pinned { "Unpin" } else { "Pin" };

        assert_eq!(pin_label, "Pin");
    }

    #[test]
    fn pinned_item_shows_unpin_label() {
        let is_pinned = true;

        let pin_label = if is_pinned { "Unpin" } else { "Pin" };

        assert_eq!(pin_label, "Unpin");
    }

    #[test]
    fn active_menu_stores_item_and_pinned_state() {
        let menu = ActiveMenu {
            item_id: "test-item".to_string(),

            is_pinned: true,

            button_rect: egui::Rect::from_min_size(
                egui::pos2(100.0, 100.0),
                egui::vec2(22.0, 22.0),
            ),
        };

        assert_eq!(menu.item_id, "test-item");

        assert!(menu.is_pinned);
    }

    #[test]
    fn header_response_default_has_no_clicks() {
        let response = HeaderResponse::default();

        assert!(!response.close_clicked);

        assert!(!response.clear_clicked);
    }

    #[test]
    fn ui_action_outcome_cleared_stores_count() {
        let outcome = UiActionOutcome::Cleared { count: 42 };

        assert_eq!(outcome, UiActionOutcome::Cleared { count: 42 });
    }
}
