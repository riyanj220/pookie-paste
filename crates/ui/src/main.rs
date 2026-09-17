mod ipc_client;
mod popup_focus;
mod popup_position;
mod theme;
mod ui_style;

use std::time::{Duration, Instant};

use eframe::egui;
use ipc::HistoryContentRef;
use theme::AppTheme;
use tokio::sync::oneshot;

const POPUP_WIDTH: f32 = 360.0;
const POPUP_HEIGHT: f32 = 420.0;
const CURSOR_OFFSET: f32 = 12.0;

const MAX_PREVIEW_LINES: usize = 3;
const MAX_CHARS_PER_LINE: usize = 70;

const IMAGE_ROW_HEIGHT: f32 = 72.0;
const IMAGE_PLACEHOLDER_SIZE: f32 = 48.0;

const FOCUS_ACQUISITION_TIMEOUT: Duration = Duration::from_millis(500);

const FOCUS_RETRY_INTERVAL: Duration = Duration::from_millis(16);

fn capture_initial_focus_target() -> Option<ipc::IpcFocusTarget> {
    let runtime = tokio::runtime::Runtime::new().ok()?;

    runtime
        .block_on(ipc_client::capture_focus_target())
        .ok()
        .flatten()
}

fn main() -> eframe::Result<()> {
    /*
     * Capture the application that currently owns focus
     * before creating the popup.
     *
     * The daemon will later use this target when an item
     * is activated so it can restore the original app.
     */
    let target_id = capture_initial_focus_target();

    let app_theme = theme::detect_system_theme();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([POPUP_WIDTH, POPUP_HEIGHT])
        .with_resizable(false)
        .with_decorations(false);

    if let Some([x, y]) = popup_position::popup_position(POPUP_WIDTH, POPUP_HEIGHT, CURSOR_OFFSET) {
        viewport = viewport.with_position([x, y]);
    }

    let options = eframe::NativeOptions {
        viewport,

        renderer: eframe::Renderer::Glow,

        ..Default::default()
    };

    eframe::run_native(
        "Pookie Paste",
        options,
        Box::new(move |cc| {
            ui_style::apply_theme(&cc.egui_ctx, app_theme);

            Ok(Box::new(PookieApp::new(target_id)))
        }),
    )
}

enum HistoryState {
    Loading,

    Loaded(Vec<ipc::HistoryItem>),

    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryRowKind<'a> {
    Text(&'a str),

    Image,

    Invalid,
}

fn history_row_kind(item: &ipc::HistoryItem) -> HistoryRowKind<'_> {
    match item.content() {
        Ok(HistoryContentRef::Text(text)) => HistoryRowKind::Text(text),

        Ok(HistoryContentRef::Image { .. }) => HistoryRowKind::Image,

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

struct PookieApp {
    history: HistoryState,

    history_receiver: Option<oneshot::Receiver<Result<Vec<ipc::HistoryItem>, String>>>,

    selected_index: Option<usize>,

    /*
     * Focus acquisition is bounded.
     *
     * We actively request focus only during the popup's
     * initial appearance. Once focus has genuinely been
     * received, ordinary focus-loss dismissal takes over.
     */
    focus_started_at: Instant,

    has_received_focus: bool,

    target_id: Option<ipc::IpcFocusTarget>,

    activation_receiver: Option<oneshot::Receiver<Result<ipc::ActivationOutcome, String>>>,

    activation_in_progress: bool,

    status_message: Option<String>,
}

impl PookieApp {
    fn new(target_id: Option<ipc::IpcFocusTarget>) -> Self {
        let (sender, receiver) = oneshot::channel();

        std::thread::spawn(move || {
            let runtime =
                tokio::runtime::Runtime::new().expect("failed to create UI Tokio runtime");

            let result = runtime.block_on(ipc_client::get_history());

            let _ = sender.send(result);
        });

        Self {
            history: HistoryState::Loading,

            history_receiver: Some(receiver),

            selected_index: None,

            focus_started_at: Instant::now(),

            has_received_focus: false,

            target_id,

            activation_receiver: None,

            activation_in_progress: false,

            status_message: None,
        }
    }

    fn ensure_popup_focus(&mut self, ui: &mut egui::Ui) {
        let focused = ui.input(|input| input.viewport().focused.unwrap_or(false));

        /*
         * Only mark focus as received when egui confirms
         * the native viewport genuinely owns focus.
         */
        if focused {
            self.has_received_focus = true;

            return;
        }

        /*
         * Once the popup has received focus once, we must
         * not try to steal it back.
         *
         * A later focus loss is intentional click-away
         * behavior and is handled in ui().
         */
        if self.has_received_focus {
            return;
        }

        /*
         * Limit active focus acquisition to the first
         * 500 ms of the popup's lifetime.
         *
         * This isn't a sleep or UX delay. We retry across
         * normal UI frames because the native X11 window
         * may not yet appear in the WM client list during
         * the very first frame.
         */
        if self.focus_started_at.elapsed() > FOCUS_ACQUISITION_TIMEOUT {
            return;
        }

        /*
         * Layer 1:
         * Ask eframe/egui to focus its viewport.
         */
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);

        /*
         * Layer 2:
         * On X11, send a WM-friendly
         * _NET_ACTIVE_WINDOW request for this UI process.
         *
         * On unsupported sessions this simply returns
         * false and causes no failure.
         */
        popup_focus::request_focus();

        /*
         * Keep generating frames briefly while focus is
         * still being acquired.
         */
        ui.ctx().request_repaint_after(FOCUS_RETRY_INTERVAL);
    }

    fn poll_history(&mut self) {
        let result = match self.history_receiver.as_mut() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),

                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,

                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.history =
                        HistoryState::Failed("history loader stopped unexpectedly".to_string());

                    self.history_receiver = None;

                    self.selected_index = None;

                    return;
                }
            },

            None => None,
        };

        if let Some(result) = result {
            match result {
                Ok(items) => {
                    self.selected_index = if items.is_empty() { None } else { Some(0) };

                    self.history = HistoryState::Loaded(items);
                }

                Err(error) => {
                    self.history = HistoryState::Failed(error);

                    self.selected_index = None;
                }
            }

            self.history_receiver = None;
        }
    }

    fn handle_keyboard_navigation(&mut self, move_up: bool, move_down: bool) -> bool {
        let HistoryState::Loaded(items) = &self.history else {
            return false;
        };

        if items.is_empty() {
            self.selected_index = None;

            return false;
        }

        let mut changed = false;

        if move_up {
            let current = self.selected_index.unwrap_or(0);

            let next = current.saturating_sub(1);

            if next != current {
                self.selected_index = Some(next);

                changed = true;
            }
        }

        if move_down {
            let current = self.selected_index.unwrap_or(0);

            let next = (current + 1).min(items.len() - 1);

            if next != current {
                self.selected_index = Some(next);

                changed = true;
            }
        }

        changed
    }

    fn start_activation_for_index(&mut self, ctx: &egui::Context, index: usize) {
        if self.activation_in_progress {
            return;
        }

        let HistoryState::Loaded(items) = &self.history else {
            return;
        };

        let Some(item) = items.get(index) else {
            return;
        };

        let id = item.id.clone();

        /*
         * IpcFocusTarget is not Copy because KDE targets
         * contain a String.
         *
         * Keep the target stored in the app and send a
         * clone to the activation worker.
         */
        let target_id = self.target_id.clone();

        let (sender, receiver) = oneshot::channel();

        self.status_message = None;

        self.activation_in_progress = true;

        self.activation_receiver = Some(receiver);

        /*
         * Hide before asking the daemon to restore the
         * original application and paste.
         */
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));

        std::thread::spawn(move || {
            let runtime =
                tokio::runtime::Runtime::new().expect("failed to create activation runtime");

            let result = runtime.block_on(ipc_client::activate_item(id, target_id));

            let _ = sender.send(result);
        });
    }

    fn start_selected_activation(&mut self, ctx: &egui::Context) {
        let Some(index) = self.selected_index else {
            return;
        };

        self.start_activation_for_index(ctx, index);
    }

    fn poll_activation(&mut self, ctx: &egui::Context) {
        let result = match self.activation_receiver.as_mut() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),

                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,

                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    Some(Err("activation worker stopped unexpectedly".to_string()))
                }
            },

            None => None,
        };

        let Some(result) = result else {
            return;
        };

        self.activation_receiver = None;

        match result {
            Ok(ipc::ActivationOutcome::Pasted) | Ok(ipc::ActivationOutcome::ClipboardUpdated) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            Ok(ipc::ActivationOutcome::PasteFailed) => {
                self.activation_in_progress = false;

                self.status_message = Some("Couldn't paste into the application.".to_string());

                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }

            Ok(ipc::ActivationOutcome::NotFound) => {
                self.activation_in_progress = false;

                self.status_message =
                    Some("This clipboard item is no longer available.".to_string());

                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }

            Ok(ipc::ActivationOutcome::UnsupportedContent) => {
                self.activation_in_progress = false;

                self.status_message = Some("This clipboard item isn't supported yet.".to_string());

                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }

            Err(_error) => {
                self.activation_in_progress = false;

                self.status_message = Some("Something went wrong.".to_string());

                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
    }
}

fn preview_text(text: &str) -> String {
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

fn render_header(ui: &mut egui::Ui, palette: ui_style::UiPalette) -> bool {
    let mut close_clicked = false;

    ui.add_space(5.0);

    ui.horizontal(|ui| {
        ui.add_space(ui_style::WINDOW_PADDING);

        ui.label(
            egui::RichText::new("Pookie Paste")
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

            let response = ui.add_sized([28.0, 28.0], close_button);

            if response.clicked() {
                close_clicked = true;
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

    close_clicked
}

fn paint_row_background(
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

fn render_text_history_row(
    ui: &mut egui::Ui,
    text: &str,
    selected: bool,
    palette: ui_style::UiPalette,
) -> egui::Response {
    let available_width = ui.available_width();

    let text_width = available_width - (ui_style::ROW_HORIZONTAL_PADDING * 2.0);

    let font_id = egui::FontId::proportional(ui_style::BODY_TEXT_SIZE);

    let galley = ui.painter().layout(
        text.to_owned(),
        font_id,
        palette.text_primary,
        text_width.max(1.0),
    );

    let desired_height = galley.size().y + (ui_style::ROW_VERTICAL_PADDING * 2.0);

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, desired_height),
        egui::Sense::click(),
    );

    paint_row_background(ui, rect, &response, selected, palette);

    let text_position = egui::pos2(
        rect.left() + ui_style::ROW_HORIZONTAL_PADDING,
        rect.top() + ui_style::ROW_VERTICAL_PADDING,
    );

    ui.painter()
        .galley(text_position, galley, palette.text_primary);

    response
}

fn render_image_history_row(
    ui: &mut egui::Ui,
    selected: bool,
    palette: ui_style::UiPalette,
) -> egui::Response {
    let available_width = ui.available_width();

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, IMAGE_ROW_HEIGHT),
        egui::Sense::click(),
    );

    paint_row_background(ui, rect, &response, selected, palette);

    /*
     * Phase 10.9 deliberately renders a lightweight image
     * placeholder only.
     *
     * Phase 10.10 will replace this box with the decoded
     * PNG thumbnail and texture cache.
     */
    let image_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.left() + ui_style::ROW_HORIZONTAL_PADDING,
            rect.center().y - (IMAGE_PLACEHOLDER_SIZE / 2.0),
        ),
        egui::vec2(IMAGE_PLACEHOLDER_SIZE, IMAGE_PLACEHOLDER_SIZE),
    );

    ui.painter()
        .rect_filled(image_rect, ui_style::ROW_CORNER_RADIUS, palette.divider);

    let icon_center = image_rect.center();

    let icon_size = IMAGE_PLACEHOLDER_SIZE * 0.42;

    let icon_rect =
        egui::Rect::from_center_size(icon_center, egui::vec2(icon_size, icon_size * 0.72));

    ui.painter().rect_stroke(
        icon_rect,
        2.0,
        egui::Stroke::new(1.5, palette.text_secondary),
        egui::StrokeKind::Inside,
    );

    let mountain_left = egui::pos2(icon_rect.left() + 3.0, icon_rect.bottom() - 3.0);

    let mountain_peak = egui::pos2(icon_rect.center().x, icon_rect.top() + 4.0);

    let mountain_right = egui::pos2(icon_rect.right() - 3.0, icon_rect.bottom() - 3.0);

    ui.painter().line_segment(
        [mountain_left, mountain_peak],
        egui::Stroke::new(1.5, palette.text_secondary),
    );

    ui.painter().line_segment(
        [mountain_peak, mountain_right],
        egui::Stroke::new(1.5, palette.text_secondary),
    );

    let label_position = egui::pos2(image_rect.right() + 10.0, rect.center().y);

    ui.painter().text(
        label_position,
        egui::Align2::LEFT_CENTER,
        "Image",
        egui::FontId::proportional(ui_style::BODY_TEXT_SIZE),
        palette.text_primary,
    );

    response
}

fn render_invalid_history_row(
    ui: &mut egui::Ui,
    selected: bool,
    palette: ui_style::UiPalette,
) -> egui::Response {
    let available_width = ui.available_width();

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(available_width, 44.0), egui::Sense::click());

    paint_row_background(ui, rect, &response, selected, palette);

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

    response
}

fn render_history_item_row(
    ui: &mut egui::Ui,
    item: &ipc::HistoryItem,
    selected: bool,
    palette: ui_style::UiPalette,
) -> egui::Response {
    match history_row_kind(item) {
        HistoryRowKind::Text(text) => {
            let preview = preview_text(text);

            render_text_history_row(ui, &preview, selected, palette)
        }

        HistoryRowKind::Image => render_image_history_row(ui, selected, palette),

        HistoryRowKind::Invalid => render_invalid_history_row(ui, selected, palette),
    }
}

fn render_state_message(
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

fn render_status_message(ui: &mut egui::Ui, message: &str, palette: ui_style::UiPalette) {
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

impl eframe::App for PookieApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        /*
         * Focus acquisition must happen before keyboard
         * input handling so arrows/Enter can work as soon
         * as the popup receives native focus.
         */
        self.ensure_popup_focus(ui);

        self.poll_history();

        self.poll_activation(ui.ctx());

        let viewport_focused = ui.input(|input| input.viewport().focused.unwrap_or(false));

        /*
         * Keep this separate from the focus-request code.
         *
         * has_received_focus means "the WM actually gave
         * us focus", not merely "we requested it".
         */
        if viewport_focused {
            self.has_received_focus = true;
        }

        /*
         * Do not close the popup merely because its first
         * frame wasn't focused yet.
         *
         * Only treat focus loss as dismissal after focus
         * has genuinely been obtained at least once.
         */
        if self.has_received_focus && !viewport_focused && !self.activation_in_progress {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);

            return;
        }

        let close_requested = ui.input(|input| input.key_pressed(egui::Key::Escape));

        if close_requested {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);

            return;
        }

        let move_up = ui.input(|input| input.key_pressed(egui::Key::ArrowUp));

        let move_down = ui.input(|input| input.key_pressed(egui::Key::ArrowDown));

        let activate = ui.input(|input| input.key_pressed(egui::Key::Enter));

        let keyboard_selection_changed = self.handle_keyboard_navigation(move_up, move_down);

        if activate {
            self.start_selected_activation(ui.ctx());
        }

        let palette = ui_style::palette(if ui.visuals().dark_mode {
            AppTheme::Dark
        } else {
            AppTheme::Light
        });

        if render_header(ui, palette) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);

            return;
        }

        if let Some(message) = &self.status_message {
            render_status_message(ui, message, palette);
        }

        let mut clicked_index = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(2.0);

                match &self.history {
                    HistoryState::Loading => {
                        render_state_message(ui, "Loading…", None, palette);
                    }

                    HistoryState::Loaded(items) => {
                        if items.is_empty() {
                            render_state_message(
                                ui,
                                "No clipboard history yet",
                                Some("Copy something to get started."),
                                palette,
                            );

                            return;
                        }

                        /*
                         * Important mixed-history
                         * invariant:
                         *
                         * every HistoryItem maps to
                         * exactly one rendered row.
                         *
                         * Do not skip image or malformed
                         * entries here. selected_index
                         * indexes this exact vector.
                         */
                        for (index, item) in items.iter().enumerate() {
                            let selected = self.selected_index == Some(index);

                            let mut row_response = None;

                            ui.horizontal(|ui| {
                                ui.add_space(ui_style::LIST_HORIZONTAL_MARGIN);

                                let remaining_width = (ui.available_width()
                                    - ui_style::LIST_HORIZONTAL_MARGIN)
                                    .max(1.0);

                                ui.allocate_ui_with_layout(
                                    egui::vec2(remaining_width, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.set_width(remaining_width);

                                        row_response = Some(render_history_item_row(
                                            ui, item, selected, palette,
                                        ));
                                    },
                                );
                            });

                            let Some(response) = row_response else {
                                continue;
                            };

                            if response.clicked() {
                                clicked_index = Some(index);
                            }

                            if selected && keyboard_selection_changed {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }

                            ui.add_space(ui_style::ROW_GAP);
                        }
                    }

                    HistoryState::Failed(_error) => {
                        render_state_message(
                            ui,
                            "Pookie Paste isn't available",
                            Some("Please try again."),
                            palette,
                        );
                    }
                }
            });

        if let Some(index) = clicked_index {
            self.selected_index = Some(index);

            self.start_activation_for_index(ui.ctx(), index);
        }
    }
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
        assert_eq!(preview_text("Hello world",), "Hello world",);
    }

    #[test]
    fn preview_limits_number_of_lines() {
        let preview = preview_text("one\ntwo\nthree\nfour");

        assert_eq!(preview, "one\ntwo\nthree…",);
    }

    #[test]
    fn preview_limits_long_lines() {
        let input = "a".repeat(MAX_CHARS_PER_LINE + 20);

        let preview = preview_text(&input);

        assert!(preview.ends_with('…',),);
    }

    #[test]
    fn text_item_maps_to_text_row() {
        let item = text_item("hello");

        assert_eq!(history_row_kind(&item,), HistoryRowKind::Text("hello",),);
    }

    #[test]
    fn image_item_maps_to_image_row() {
        let item = image_item();

        assert_eq!(history_row_kind(&item,), HistoryRowKind::Image,);
    }

    #[test]
    fn malformed_item_still_maps_to_visible_row_kind() {
        let item = ipc::HistoryItem {
            id: "broken".to_string(),

            content_type: "image".to_string(),

            text_content: None,

            file_path: None,

            created_at: "2026-09-17T10:00:00Z".to_string(),
        };

        assert_eq!(history_row_kind(&item,), HistoryRowKind::Invalid,);
    }

    #[test]
    fn mixed_items_preserve_vector_index_mapping() {
        let items = [text_item("first"), image_item(), text_item("third")];

        assert_eq!(history_row_kind(&items[0],), HistoryRowKind::Text("first",),);

        assert_eq!(history_row_kind(&items[1],), HistoryRowKind::Image,);

        assert_eq!(history_row_kind(&items[2],), HistoryRowKind::Text("third",),);
    }
}
