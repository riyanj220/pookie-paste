use eframe::egui;

use crate::ui_style;

pub(crate) fn render_overflow_icon(ui: &egui::Ui, center: egui::Pos2, color: egui::Color32) {
    let radius = 1.8;

    let spacing = 4.2;

    let top = egui::pos2(center.x, center.y - spacing);

    let mid = center;

    let bottom = egui::pos2(center.x, center.y + spacing);

    ui.painter().circle_filled(top, radius, color);

    ui.painter().circle_filled(mid, radius, color);

    ui.painter().circle_filled(bottom, radius, color);
}

pub(crate) fn render_pin_icon(ui: &egui::Ui, center: egui::Pos2, color: egui::Color32) {
    let cap_left = egui::pos2(center.x - 2.5, center.y - 4.5);

    let cap_right = egui::pos2(center.x + 2.5, center.y - 4.5);

    let head_bottom_right = egui::pos2(center.x + 1.2, center.y - 1.0);

    let head_bottom_left = egui::pos2(center.x - 1.2, center.y - 1.0);

    ui.painter().add(egui::Shape::convex_polygon(
        vec![cap_left, cap_right, head_bottom_right, head_bottom_left],
        color,
        egui::Stroke::NONE,
    ));

    let guard_left = egui::pos2(center.x - 3.2, center.y - 1.0);

    let guard_right = egui::pos2(center.x + 3.2, center.y - 1.0);

    ui.painter()
        .line_segment([guard_left, guard_right], egui::Stroke::new(1.4, color));

    let needle_bottom = egui::pos2(center.x, center.y + 4.5);

    ui.painter().line_segment(
        [egui::pos2(center.x, center.y - 1.0), needle_bottom],
        egui::Stroke::new(1.2, color),
    );
}

pub(crate) fn render_card_controls(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    is_pinned: bool,
    is_menu_open: bool,
    palette: ui_style::UiPalette,
) -> (bool, egui::Rect) {
    let button_size = egui::vec2(22.0, 22.0);

    let button_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.right() - ui_style::ROW_HORIZONTAL_PADDING - button_size.x,
            rect.top() + ui_style::ROW_VERTICAL_PADDING,
        ),
        button_size,
    );

    let button_response = ui
        .allocate_rect(button_rect, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if button_response.hovered() || is_menu_open {
        let hover_bg = if ui.visuals().dark_mode {
            egui::Color32::from_white_alpha(24)
        } else {
            egui::Color32::from_black_alpha(18)
        };

        ui.painter()
            .circle_filled(button_rect.center(), 11.0, hover_bg);
    }

    let dot_color = if button_response.hovered() || is_menu_open {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    render_overflow_icon(ui, button_rect.center(), dot_color);

    if is_pinned {
        let pin_center = egui::pos2(button_rect.left() - 11.0, button_rect.center().y);

        render_pin_icon(ui, pin_center, palette.text_secondary);
    }

    (button_response.clicked(), button_rect)
}

pub(crate) fn render_menu_item(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    palette: ui_style::UiPalette,
) -> egui::Response {
    let response = ui
        .allocate_rect(rect, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if response.hovered() {
        ui.painter()
            .rect_filled(rect, ui_style::ROW_CORNER_RADIUS, palette.row_hover);
    }

    let text_color = if response.hovered() {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    ui.painter().text(
        egui::pos2(rect.left() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(ui_style::BODY_TEXT_SIZE - 1.0),
        text_color,
    );

    response
}
