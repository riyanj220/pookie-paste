use eframe::egui;

use crate::theme::AppTheme;

pub const WINDOW_PADDING: f32 = 10.0;

pub const ROW_HORIZONTAL_PADDING: f32 = 10.0;
pub const ROW_VERTICAL_PADDING: f32 = 6.0;
pub const ROW_GAP: f32 = 3.0;
pub const LIST_HORIZONTAL_MARGIN: f32 = 6.0;

pub const HEADER_TEXT_SIZE: f32 = 17.0;
pub const BODY_TEXT_SIZE: f32 = 14.0;

pub const BORDER_WIDTH: f32 = 1.0;
pub const ROW_CORNER_RADIUS: f32 = 4.0;

pub const SCROLLBAR_WIDTH: f32 = 7.0;
pub const SCROLLBAR_MIN_HANDLE_LENGTH: f32 = 24.0;

#[derive(Debug, Clone, Copy)]
pub struct UiPalette {
    pub background: egui::Color32,
    pub row_background: egui::Color32,
    pub row_hover: egui::Color32,
    pub row_selected: egui::Color32,

    pub text_primary: egui::Color32,
    pub text_secondary: egui::Color32,

    pub divider: egui::Color32,
    pub border: egui::Color32,

    pub accent: egui::Color32,
}

impl UiPalette {
    fn light() -> Self {
        Self {
            background: egui::Color32::from_rgb(248, 248, 248),

            row_background: egui::Color32::from_rgb(255, 255, 255),

            row_hover: egui::Color32::from_rgb(240, 240, 240),

            row_selected: egui::Color32::from_rgb(232, 241, 249),

            text_primary: egui::Color32::from_rgb(28, 28, 28),

            text_secondary: egui::Color32::from_rgb(100, 100, 100),

            divider: egui::Color32::from_rgb(220, 220, 220),

            border: egui::Color32::from_rgb(200, 200, 200),

            accent: egui::Color32::from_rgb(0, 120, 212),
        }
    }

    fn dark() -> Self {
        Self {
            background: egui::Color32::from_rgb(28, 28, 28),

            row_background: egui::Color32::from_rgb(32, 32, 32),

            row_hover: egui::Color32::from_rgb(43, 43, 43),

            row_selected: egui::Color32::from_rgb(40, 50, 60),

            text_primary: egui::Color32::from_rgb(242, 242, 242),

            text_secondary: egui::Color32::from_rgb(165, 165, 165),

            divider: egui::Color32::from_rgb(55, 55, 55),

            border: egui::Color32::from_rgb(70, 70, 70),

            accent: egui::Color32::from_rgb(96, 165, 250),
        }
    }
}

pub fn palette(theme: AppTheme) -> UiPalette {
    match theme {
        AppTheme::Light => UiPalette::light(),
        AppTheme::Dark => UiPalette::dark(),
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: AppTheme) {
    let palette = palette(theme);

    let mut visuals = match theme {
        AppTheme::Light => egui::Visuals::light(),
        AppTheme::Dark => egui::Visuals::dark(),
    };

    visuals.panel_fill = palette.background;

    visuals.window_fill = palette.background;

    visuals.faint_bg_color = palette.row_hover;

    visuals.extreme_bg_color = palette.row_background;

    visuals.selection.bg_fill = palette.row_selected;

    visuals.selection.stroke = egui::Stroke::new(1.0, palette.accent);

    visuals.widgets.hovered.bg_fill = palette.row_hover;

    visuals.widgets.hovered.weak_bg_fill = palette.row_hover;

    visuals.widgets.active.bg_fill = palette.row_selected;

    visuals.widgets.active.weak_bg_fill = palette.row_selected;

    visuals.window_stroke = egui::Stroke::new(BORDER_WIDTH, palette.border);

    ctx.set_visuals(visuals);

    let egui_theme = match theme {
        AppTheme::Light => egui::Theme::Light,
        AppTheme::Dark => egui::Theme::Dark,
    };

    let mut style = (*ctx.style_of(egui_theme)).clone();

    style.spacing.scroll.bar_width = SCROLLBAR_WIDTH;

    style.spacing.scroll.handle_min_length = SCROLLBAR_MIN_HANDLE_LENGTH;

    ctx.set_style_of(egui_theme, style);
}
