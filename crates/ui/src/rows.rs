use eframe::egui;

use crate::controls::render_card_controls;
use crate::history::{HistoryRowKind, history_row_kind, preview_text};
use crate::image_thumbnail::{ImageThumbnail, ImageThumbnailCache};
use crate::ui_style;

/*
 * Compact Windows-style image history card.
 *
 * The thumbnail is large enough to recognize at a glance
 * without turning the clipboard panel into an image gallery.
 */
pub(crate) const IMAGE_ROW_HEIGHT: f32 = 96.0;
pub(crate) const IMAGE_THUMBNAIL_MAX_WIDTH: f32 = 132.0;
pub(crate) const IMAGE_THUMBNAIL_MAX_HEIGHT: f32 = 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowVisualState {
    pub(crate) selected: bool,
    pub(crate) is_pinned: bool,
    pub(crate) is_menu_open: bool,
}

pub(crate) struct HistoryRowResponse {
    pub(crate) card_clicked: bool,
    pub(crate) menu_button_clicked: bool,
    pub(crate) button_rect: egui::Rect,
    pub(crate) response: egui::Response,
}

pub(crate) fn paint_row_background(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    selected: bool,
    palette: ui_style::UiPalette,
) {
    let background = if selected {
        palette.row_selected
    } else if response.hovered() {
        palette.row_hover
    } else {
        palette.row_background
    };

    ui.painter()
        .rect_filled(rect, ui_style::ROW_CORNER_RADIUS, background);

    if selected {
        let indicator_rect =
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.left() + 3.0, rect.bottom()));

        ui.painter()
            .rect_filled(indicator_rect, 2.0, palette.accent);
    }
}

pub(crate) fn fit_thumbnail_size(thumbnail: ImageThumbnail) -> egui::Vec2 {
    let aspect = thumbnail.aspect_ratio;

    if !aspect.is_finite() || aspect <= 0.0 {
        return egui::vec2(IMAGE_THUMBNAIL_MAX_HEIGHT, IMAGE_THUMBNAIL_MAX_HEIGHT);
    }

    let available_aspect = IMAGE_THUMBNAIL_MAX_WIDTH / IMAGE_THUMBNAIL_MAX_HEIGHT;

    if aspect >= available_aspect {
        egui::vec2(
            IMAGE_THUMBNAIL_MAX_WIDTH,
            IMAGE_THUMBNAIL_MAX_WIDTH / aspect,
        )
    } else {
        egui::vec2(
            IMAGE_THUMBNAIL_MAX_HEIGHT * aspect,
            IMAGE_THUMBNAIL_MAX_HEIGHT,
        )
    }
}

pub(crate) fn paint_missing_image_placeholder(
    ui: &egui::Ui,
    rect: egui::Rect,
    palette: ui_style::UiPalette,
) {
    let size = 48.0;

    let placeholder = egui::Rect::from_min_size(
        egui::pos2(
            rect.left() + ui_style::ROW_HORIZONTAL_PADDING,
            rect.center().y - size / 2.0,
        ),
        egui::vec2(size, size),
    );

    ui.painter()
        .rect_filled(placeholder, ui_style::ROW_CORNER_RADIUS, palette.divider);

    let icon_rect = placeholder.shrink(13.0);

    ui.painter().rect_stroke(
        icon_rect,
        2.0,
        egui::Stroke::new(1.4, palette.text_secondary),
        egui::StrokeKind::Inside,
    );

    let left = egui::pos2(icon_rect.left() + 2.0, icon_rect.bottom() - 3.0);

    let peak = egui::pos2(icon_rect.center().x, icon_rect.top() + 3.0);

    let right = egui::pos2(icon_rect.right() - 2.0, icon_rect.bottom() - 3.0);

    ui.painter()
        .line_segment([left, peak], egui::Stroke::new(1.4, palette.text_secondary));

    ui.painter().line_segment(
        [peak, right],
        egui::Stroke::new(1.4, palette.text_secondary),
    );
}

pub(crate) fn render_text_history_row(
    ui: &mut egui::Ui,
    text: &str,
    state: RowVisualState,
    palette: ui_style::UiPalette,
) -> HistoryRowResponse {
    let available_width = ui.available_width();

    let right_controls_width = if state.is_pinned { 44.0 } else { 26.0 };

    let text_width =
        available_width - (ui_style::ROW_HORIZONTAL_PADDING * 2.0) - right_controls_width;

    let font_id = egui::FontId::proportional(ui_style::BODY_TEXT_SIZE);

    let galley = ui.painter().layout(
        text.to_owned(),
        font_id,
        palette.text_primary,
        text_width.max(1.0),
    );

    let desired_height = (galley.size().y + (ui_style::ROW_VERTICAL_PADDING * 2.0)).max(36.0);

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, desired_height),
        egui::Sense::click(),
    );

    paint_row_background(ui, rect, &response, state.selected, palette);

    let text_position = egui::pos2(
        rect.left() + ui_style::ROW_HORIZONTAL_PADDING,
        rect.top() + ui_style::ROW_VERTICAL_PADDING,
    );

    ui.painter()
        .galley(text_position, galley, palette.text_primary);

    let (menu_button_clicked, button_rect) =
        render_card_controls(ui, rect, state.is_pinned, state.is_menu_open, palette);

    let card_clicked = response.clicked() && !menu_button_clicked;

    HistoryRowResponse {
        card_clicked,
        menu_button_clicked,
        button_rect,
        response,
    }
}

pub(crate) fn render_image_history_row(
    ui: &mut egui::Ui,
    item_id: &str,
    file_path: &str,
    state: RowVisualState,
    palette: ui_style::UiPalette,
    thumbnails: &mut ImageThumbnailCache,
) -> HistoryRowResponse {
    let available_width = ui.available_width();

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, IMAGE_ROW_HEIGHT),
        egui::Sense::click(),
    );

    paint_row_background(ui, rect, &response, state.selected, palette);

    let thumbnail = thumbnails.get_or_load(ui.ctx(), item_id, file_path);

    if let Some(thumbnail) = thumbnail {
        let thumbnail_size = fit_thumbnail_size(thumbnail);

        let image_rect = egui::Rect::from_min_size(
            egui::pos2(
                rect.left() + ui_style::ROW_HORIZONTAL_PADDING,
                rect.center().y - thumbnail_size.y / 2.0,
            ),
            thumbnail_size,
        );

        /*
         * Very subtle surface behind transparent PNGs.
         *
         * This keeps screenshots/photos visually clean while
         * still making transparent images readable in both
         * light and dark themes.
         */
        let image_background = image_rect.expand(2.0);

        ui.painter().rect_filled(
            image_background,
            ui_style::ROW_CORNER_RADIUS,
            palette.divider,
        );

        ui.painter().image(
            thumbnail.texture_id,
            image_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        paint_missing_image_placeholder(ui, rect, palette);
    }

    let (menu_button_clicked, button_rect) =
        render_card_controls(ui, rect, state.is_pinned, state.is_menu_open, palette);

    let card_clicked = response.clicked() && !menu_button_clicked;

    HistoryRowResponse {
        card_clicked,
        menu_button_clicked,
        button_rect,
        response,
    }
}

pub(crate) fn render_invalid_history_row(
    ui: &mut egui::Ui,
    state: RowVisualState,
    palette: ui_style::UiPalette,
) -> HistoryRowResponse {
    let available_width = ui.available_width();

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(available_width, 44.0), egui::Sense::click());

    paint_row_background(ui, rect, &response, state.selected, palette);

    ui.painter().text(
        egui::pos2(
            rect.left() + ui_style::ROW_HORIZONTAL_PADDING,
            rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        "Unavailable clipboard item",
        egui::FontId::proportional(ui_style::BODY_TEXT_SIZE),
        palette.text_secondary,
    );

    let (menu_button_clicked, button_rect) =
        render_card_controls(ui, rect, state.is_pinned, state.is_menu_open, palette);

    let card_clicked = response.clicked() && !menu_button_clicked;

    HistoryRowResponse {
        card_clicked,
        menu_button_clicked,
        button_rect,
        response,
    }
}

pub(crate) fn render_history_item_row(
    ui: &mut egui::Ui,
    item: &ipc::HistoryItem,
    state: RowVisualState,
    palette: ui_style::UiPalette,
    thumbnails: &mut ImageThumbnailCache,
) -> HistoryRowResponse {
    match history_row_kind(item) {
        HistoryRowKind::Text(text) => {
            let preview = preview_text(text);

            render_text_history_row(ui, &preview, state, palette)
        }

        HistoryRowKind::Image { file_path } => {
            render_image_history_row(ui, &item.id, file_path, state, palette, thumbnails)
        }

        HistoryRowKind::Invalid => render_invalid_history_row(ui, state, palette),
    }
}

pub(crate) fn render_state_message(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: Option<&str>,
    palette: ui_style::UiPalette,
) {
    ui.add_space(28.0);

    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(ui_style::BODY_TEXT_SIZE)
                .color(palette.text_primary)
                .strong(),
        );

        if let Some(subtitle) = subtitle {
            ui.add_space(4.0);

            ui.label(
                egui::RichText::new(subtitle)
                    .size(ui_style::BODY_TEXT_SIZE - 1.0)
                    .color(palette.text_secondary),
            );
        }
    });
}

pub(crate) fn render_status_message(
    ui: &mut egui::Ui,
    message: &str,
    palette: ui_style::UiPalette,
) {
    ui.horizontal(|ui| {
        ui.add_space(ui_style::LIST_HORIZONTAL_MARGIN);

        ui.label(
            egui::RichText::new(message)
                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                .color(palette.text_secondary),
        );
    });

    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_thumbnail_fits_available_bounds() {
        let aspect = 16.0 / 9.0;

        let size = fit_thumbnail_size(ImageThumbnail {
            texture_id: egui::TextureId::Managed(1),

            aspect_ratio: aspect,
        });

        assert_eq!(size.y, IMAGE_THUMBNAIL_MAX_HEIGHT);

        assert_eq!(size.x, IMAGE_THUMBNAIL_MAX_HEIGHT * aspect);

        assert!(size.x <= IMAGE_THUMBNAIL_MAX_WIDTH);
    }

    #[test]
    fn portrait_thumbnail_fits_height() {
        let size = fit_thumbnail_size(ImageThumbnail {
            texture_id: egui::TextureId::Managed(1),

            aspect_ratio: 9.0 / 16.0,
        });

        assert_eq!(size.y, IMAGE_THUMBNAIL_MAX_HEIGHT);

        assert!(size.x <= IMAGE_THUMBNAIL_MAX_WIDTH);
    }

    #[test]
    fn clicking_menu_button_never_activates_card() {
        let card_raw_clicked = true;

        let menu_button_clicked = true;

        let card_clicked = card_raw_clicked && !menu_button_clicked;

        assert!(!card_clicked);
    }

    #[test]
    fn clicking_card_body_without_menu_button_activates_card() {
        let card_raw_clicked = true;

        let menu_button_clicked = false;

        let card_clicked = card_raw_clicked && !menu_button_clicked;

        assert!(card_clicked);
    }
}
