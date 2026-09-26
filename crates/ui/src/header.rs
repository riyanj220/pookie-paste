use eframe::egui;

use crate::actions::HeaderResponse;
use crate::ui_style;

fn render_header_clear_button(ui: &mut egui::Ui, palette: ui_style::UiPalette) -> bool {
    let text = "Clear";

    let font_id = egui::FontId::proportional(ui_style::BODY_TEXT_SIZE - 2.0);

    let galley =
        ui.painter()
            .layout_no_wrap(text.to_string(), font_id.clone(), palette.text_secondary);

    let button_size = egui::vec2(galley.size().x + 12.0, 24.0);

    let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    if response.hovered() {
        let hover_bg = if ui.visuals().dark_mode {
            egui::Color32::from_white_alpha(20)
        } else {
            egui::Color32::from_black_alpha(15)
        };

        ui.painter()
            .rect_filled(rect, ui_style::ROW_CORNER_RADIUS, hover_bg);
    }

    let text_color = if response.hovered() {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        font_id,
        text_color,
    );

    response.clicked()
}

pub(crate) fn render_header(
    ui: &mut egui::Ui,
    palette: ui_style::UiPalette,
    can_clear: bool,
    in_settings: bool,
) -> HeaderResponse {
    let mut response = HeaderResponse::default();

    ui.add_space(5.0);

    ui.horizontal(|ui| {
        ui.add_space(ui_style::WINDOW_PADDING);

        let title = if in_settings {
            "Shortcut Setup"
        } else {
            "Pookie Paste"
        };

        ui.label(
            egui::RichText::new(title)
                .size(ui_style::HEADER_TEXT_SIZE)
                .color(palette.text_primary)
                .strong(),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(ui_style::WINDOW_PADDING);

            let close_button = egui::Button::new(
                egui::RichText::new("×")
                    .size(16.0)
                    .color(palette.text_secondary),
            )
            .frame(false);

            let close_res = ui
                .add_sized([28.0, 28.0], close_button)
                .on_hover_cursor(egui::CursorIcon::PointingHand);

            if close_res.clicked() {
                response.close_clicked = true;
            }

            ui.add_space(2.0);

            let gear_color = if in_settings {
                palette.accent
            } else {
                palette.text_secondary
            };

            let gear_button =
                egui::Button::new(egui::RichText::new("⚙").size(14.0).color(gear_color))
                    .frame(false);

            let gear_tooltip = if in_settings {
                "Back to clipboard history"
            } else {
                "Shortcut setup"
            };

            let gear_res = ui
                .add_sized([28.0, 28.0], gear_button)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(gear_tooltip);

            if gear_res.clicked() {
                response.settings_clicked = true;
            }

            if can_clear && !in_settings {
                ui.add_space(6.0);

                if render_header_clear_button(ui, palette) {
                    response.clear_clicked = true;
                }
            }
        });
    });

    ui.add_space(3.0);

    let width = ui.available_width();

    let start = ui.cursor().min;

    let end = egui::pos2(start.x + width, start.y);

    ui.painter()
        .line_segment([start, end], egui::Stroke::new(1.0, palette.divider));

    ui.add_space(5.0);

    response
}
