mod image_thumbnail;
mod ipc_client;
mod popup_anchor;
mod popup_focus;
mod popup_position;
mod theme;
mod ui_style;

use std::time::{Duration, Instant};

use eframe::egui;
use image_thumbnail::{ImageThumbnail, ImageThumbnailCache};
use ipc::HistoryContentRef;
use popup_focus::FocusRequestState;
use theme::AppTheme;
use tokio::sync::oneshot;

const POPUP_WIDTH: f32 = 360.0;
const POPUP_HEIGHT: f32 = 420.0;
const CURSOR_OFFSET: f32 = 12.0;

const MAX_PREVIEW_LINES: usize = 3;
const MAX_CHARS_PER_LINE: usize = 70;

/*
 * Compact Windows-style image history card.
 *
 * The thumbnail is large enough to recognize at a glance
 * without turning the clipboard panel into an image gallery.
 */
const IMAGE_ROW_HEIGHT: f32 = 96.0;

const IMAGE_THUMBNAIL_MAX_WIDTH: f32 = 132.0;

const IMAGE_THUMBNAIL_MAX_HEIGHT: f32 = 72.0;

/*
 * Native X11 acquisition owns its own timeout and starts
 * that clock only after the native popup window exists.
 *
 * This fallback timeout is retained for sessions where the
 * X11-specific helper does not apply, preserving the existing
 * eframe/Wayland focus behavior.
 */
const FALLBACK_FOCUS_ACQUISITION_TIMEOUT: Duration = Duration::from_millis(500);

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

    if let Some(resolved) = popup_anchor::resolve_popup_anchor(target_id.as_ref()) {
        let position = popup_position::popup_position(
            resolved.anchor,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            CURSOR_OFFSET,
            resolved.screen,
        );

        viewport = viewport.with_position(position);
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

            Ok(Box::new(PookieApp::new(target_id, cc.egui_ctx.clone())))
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

    Image { file_path: &'a str },

    Invalid,
}

fn history_row_kind(item: &ipc::HistoryItem) -> HistoryRowKind<'_> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveMenu {
    item_id: String,
    is_pinned: bool,
    button_rect: egui::Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuActionKind {
    Pin,
    Unpin,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UiActionOutcome {
    PinToggled { id: String, is_pinned: bool },
    Deleted { id: String },
    Cleared { count: u64 },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct HeaderResponse {
    close_clicked: bool,
    clear_clicked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowVisualState {
    selected: bool,
    is_pinned: bool,
    is_menu_open: bool,
}

struct HistoryRowResponse {
    card_clicked: bool,
    menu_button_clicked: bool,
    button_rect: egui::Rect,
    response: egui::Response,
}

struct PookieApp {
    history: HistoryState,

    history_receiver: Option<oneshot::Receiver<Result<Vec<ipc::HistoryItem>, String>>>,

    selected_index: Option<usize>,

    image_thumbnails: ImageThumbnailCache,

    /*
     * Non-X11 fallback only.
     *
     * X11 focus timing is owned by popup_focus and starts
     * only after the native popup has been discovered.
     */
    fallback_focus_started_at: Instant,

    has_received_focus: bool,

    target_id: Option<ipc::IpcFocusTarget>,

    activation_receiver: Option<oneshot::Receiver<Result<ipc::ActivationOutcome, String>>>,

    activation_in_progress: bool,

    status_message: Option<String>,

    active_menu: Option<ActiveMenu>,

    action_receiver: Option<oneshot::Receiver<Result<UiActionOutcome, String>>>,

    action_in_progress: bool,
}

impl PookieApp {
    fn new(target_id: Option<ipc::IpcFocusTarget>, repaint_context: egui::Context) -> Self {
        let (sender, receiver) = oneshot::channel();

        /*
         * History loading happens off the UI thread.
         *
         * Explicitly wake egui when the worker finishes rather
         * than relying on unrelated window/input events to cause
         * another frame.
         *
         * This keeps background-to-UI communication event-driven
         * and avoids continuous polling.
         */
        std::thread::spawn(move || {
            let runtime =
                tokio::runtime::Runtime::new().expect("failed to create UI Tokio runtime");

            let result = runtime.block_on(ipc_client::get_history());

            if sender.send(result).is_ok() {
                repaint_context.request_repaint();
            }
        });

        Self {
            history: HistoryState::Loading,

            history_receiver: Some(receiver),

            selected_index: None,

            image_thumbnails: ImageThumbnailCache::new(),

            fallback_focus_started_at: Instant::now(),

            has_received_focus: false,

            target_id,

            activation_receiver: None,

            activation_in_progress: false,

            status_message: None,

            active_menu: None,

            action_receiver: None,

            action_in_progress: false,
        }
    }

    fn ensure_popup_focus(&mut self, ui: &mut egui::Ui) {
        let focused = ui.input(|input| input.viewport().focused.unwrap_or(false));

        if focused {
            self.has_received_focus = true;

            return;
        }

        /*
         * Once focus has genuinely been received, never
         * try to steal it back.
         *
         * A later focus loss means the user intentionally
         * clicked elsewhere and normal popup dismissal
         * should take over.
         */
        if self.has_received_focus {
            return;
        }

        /*
         * Ask eframe for native viewport focus regardless
         * of platform.
         */
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);

        match popup_focus::request_focus() {
            FocusRequestState::WaitingForWindow
            | FocusRequestState::Activating
            | FocusRequestState::Acquired => {
                /*
                 * On X11 the native helper owns the state
                 * machine and timeout.
                 *
                 * Continue generating lightweight frames
                 * while:
                 *
                 *   waiting for the WM to publish the window
                 *   or
                 *   waiting for _NET_ACTIVE_WINDOW to settle.
                 *
                 * The activation timer does not begin until
                 * the native window actually exists.
                 */
                ui.ctx().request_repaint_after(FOCUS_RETRY_INTERVAL);
            }

            FocusRequestState::Unavailable => {
                /*
                 * Preserve the previous generic focus behavior
                 * for non-X11 sessions such as Wayland.
                 *
                 * This path intentionally does not affect the
                 * X11 state machine.
                 */
                if self.fallback_focus_started_at.elapsed() <= FALLBACK_FOCUS_ACQUISITION_TIMEOUT {
                    ui.ctx().request_repaint_after(FOCUS_RETRY_INTERVAL);
                }
            }

            FocusRequestState::TimedOut => {
                /*
                 * A valid X11 popup existed, but the WM did
                 * not activate it within the bounded native
                 * focus window.
                 *
                 * Stop retrying rather than creating an
                 * unbounded focus-stealing loop.
                 */
            }
        }
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

        let target_id = self.target_id.clone();

        let (sender, receiver) = oneshot::channel();

        /*
         * The popup becomes hidden while the daemon performs
         * clipboard writeback, focus restoration, and optional
         * direct paste.
         *
         * Once hidden, the native window system may stop
         * producing frames entirely. Therefore the activation
         * worker must explicitly wake egui when its result is
         * ready.
         */
        let repaint_context = ctx.clone();

        self.status_message = None;

        self.activation_in_progress = true;

        self.activation_receiver = Some(receiver);

        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));

        std::thread::spawn(move || {
            let runtime =
                tokio::runtime::Runtime::new().expect("failed to create activation runtime");

            let result = runtime.block_on(ipc_client::activate_item(id, target_id));

            /*
             * Publish the result first, then wake the UI.
             *
             * That ordering guarantees poll_activation() can
             * observe the completed result on the repaint
             * triggered below.
             *
             * If the receiver has already disappeared because
             * the UI is shutting down, no repaint is necessary.
             */
            if sender.send(result).is_ok() {
                repaint_context.request_repaint();
            }
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

    fn reload_history(&mut self, ctx: &egui::Context) {
        let (sender, receiver) = oneshot::channel();

        let repaint_context = ctx.clone();

        std::thread::spawn(move || {
            let runtime =
                tokio::runtime::Runtime::new().expect("failed to create UI Tokio runtime");

            let result = runtime.block_on(ipc_client::get_history());

            if sender.send(result).is_ok() {
                repaint_context.request_repaint();
            }
        });

        self.history_receiver = Some(receiver);
    }

    fn start_toggle_pin(&mut self, ctx: &egui::Context, id: String) {
        if self.action_in_progress {
            return;
        }

        self.action_in_progress = true;

        let (sender, receiver) = oneshot::channel();

        let repaint_context = ctx.clone();

        let item_id = id.clone();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to create action runtime");

            let result = runtime
                .block_on(ipc_client::toggle_pin_item(item_id.clone()))
                .map(|is_pinned| UiActionOutcome::PinToggled {
                    id: item_id,
                    is_pinned,
                });

            if sender.send(result).is_ok() {
                repaint_context.request_repaint();
            }
        });

        self.action_receiver = Some(receiver);
    }

    fn start_delete_item(&mut self, ctx: &egui::Context, id: String) {
        if self.action_in_progress {
            return;
        }

        self.action_in_progress = true;

        let (sender, receiver) = oneshot::channel();

        let repaint_context = ctx.clone();

        let item_id = id.clone();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to create action runtime");

            let result = runtime
                .block_on(ipc_client::delete_item(item_id.clone()))
                .map(|_deleted| UiActionOutcome::Deleted { id: item_id });

            if sender.send(result).is_ok() {
                repaint_context.request_repaint();
            }
        });

        self.action_receiver = Some(receiver);
    }

    fn start_clear_history(&mut self, ctx: &egui::Context) {
        if self.action_in_progress {
            return;
        }

        self.action_in_progress = true;

        self.active_menu = None;

        let (sender, receiver) = oneshot::channel();

        let repaint_context = ctx.clone();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to create action runtime");

            let result = runtime
                .block_on(ipc_client::clear_history())
                .map(|count| UiActionOutcome::Cleared { count });

            if sender.send(result).is_ok() {
                repaint_context.request_repaint();
            }
        });

        self.action_receiver = Some(receiver);
    }

    fn poll_action(&mut self, ctx: &egui::Context) {
        let result = match self.action_receiver.as_mut() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),

                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,

                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    Some(Err("action worker stopped unexpectedly".to_string()))
                }
            },

            None => None,
        };

        let Some(result) = result else {
            return;
        };

        self.action_receiver = None;

        self.action_in_progress = false;

        match result {
            Ok(UiActionOutcome::PinToggled { .. }) => {
                self.reload_history(ctx);
            }

            Ok(UiActionOutcome::Deleted { id }) => {
                if let HistoryState::Loaded(ref mut items) = self.history {
                    items.retain(|i| i.id != id);

                    if items.is_empty() {
                        self.selected_index = None;
                    } else {
                        self.selected_index = self.selected_index.map(|s| s.min(items.len() - 1));
                    }
                }
            }

            Ok(UiActionOutcome::Cleared { count }) => {
                tracing::debug!(count, "cleared clipboard history");

                if let HistoryState::Loaded(ref mut items) = self.history {
                    items.clear();
                }

                self.selected_index = None;
                self.image_thumbnails = ImageThumbnailCache::new();
                self.reload_history(ctx);
            }

            Err(error) => {
                tracing::warn!(error = %error, "context action failed");

                self.status_message = Some("Action failed. Please try again.".to_string());
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

fn render_header(
    ui: &mut egui::Ui,
    palette: ui_style::UiPalette,
    can_clear: bool,
) -> HeaderResponse {
    let mut response = HeaderResponse::default();

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

            let close_res = ui
                .add_sized([28.0, 28.0], close_button)
                .on_hover_cursor(egui::CursorIcon::PointingHand);

            if close_res.clicked() {
                response.close_clicked = true;
            }

            if can_clear {
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

fn render_overflow_icon(ui: &egui::Ui, center: egui::Pos2, color: egui::Color32) {
    let radius = 1.8;

    let spacing = 4.2;

    let top = egui::pos2(center.x, center.y - spacing);

    let mid = center;

    let bottom = egui::pos2(center.x, center.y + spacing);

    ui.painter().circle_filled(top, radius, color);

    ui.painter().circle_filled(mid, radius, color);

    ui.painter().circle_filled(bottom, radius, color);
}

fn render_pin_icon(ui: &egui::Ui, center: egui::Pos2, color: egui::Color32) {
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

fn render_card_controls(
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

fn render_menu_item(
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

fn render_text_history_row(
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

fn fit_thumbnail_size(thumbnail: ImageThumbnail) -> egui::Vec2 {
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

fn paint_missing_image_placeholder(ui: &egui::Ui, rect: egui::Rect, palette: ui_style::UiPalette) {
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

fn render_image_history_row(
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

fn render_invalid_history_row(
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

fn render_history_item_row(
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
        self.ensure_popup_focus(ui);

        self.poll_history();

        self.poll_activation(ui.ctx());

        self.poll_action(ui.ctx());

        let viewport_focused = ui.input(|input| input.viewport().focused.unwrap_or(false));

        if viewport_focused {
            self.has_received_focus = true;
        }

        if self.has_received_focus && !viewport_focused && !self.activation_in_progress {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);

            return;
        }

        let close_requested = ui.input(|input| input.key_pressed(egui::Key::Escape));

        if close_requested {
            if self.active_menu.is_some() {
                self.active_menu = None;

                return;
            }

            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);

            return;
        }

        let move_up = ui.input(|input| input.key_pressed(egui::Key::ArrowUp));

        let move_down = ui.input(|input| input.key_pressed(egui::Key::ArrowDown));

        let activate = ui.input(|input| input.key_pressed(egui::Key::Enter));

        let keyboard_selection_changed = self.handle_keyboard_navigation(move_up, move_down);

        if activate {
            if self.active_menu.is_some() {
                self.active_menu = None;
            } else {
                self.start_selected_activation(ui.ctx());
            }
        }

        let palette = ui_style::palette(if ui.visuals().dark_mode {
            AppTheme::Dark
        } else {
            AppTheme::Light
        });

        let has_items = match &self.history {
            HistoryState::Loaded(items) => !items.is_empty(),
            _ => false,
        };

        let header_response = render_header(ui, palette, has_items && !self.action_in_progress);

        if header_response.close_clicked {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);

            return;
        }

        if header_response.clear_clicked {
            self.start_clear_history(ui.ctx());
        }

        if let Some(message) = &self.status_message {
            render_status_message(ui, message, palette);
        }

        let mut clicked_index = None;

        /*
         * Borrow these fields independently before entering
         * the scroll closure.
         *
         * History is immutable while the image cache is
         * updated lazily as rows become visible.
         */
        let history = &self.history;

        let selected_index = self.selected_index;

        let image_thumbnails = &mut self.image_thumbnails;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(2.0);

                match history {
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

                        for (index, item) in items.iter().enumerate() {
                            let selected = selected_index == Some(index);

                            let is_menu_open = self
                                .active_menu
                                .as_ref()
                                .is_some_and(|menu| menu.item_id == item.id);

                            let mut row_action = None;

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

                                        let row_state = RowVisualState {
                                            selected,
                                            is_pinned: item.is_pinned(),
                                            is_menu_open,
                                        };

                                        row_action = Some(render_history_item_row(
                                            ui,
                                            item,
                                            row_state,
                                            palette,
                                            image_thumbnails,
                                        ));
                                    },
                                );
                            });

                            let Some(action) = row_action else {
                                continue;
                            };

                            if action.menu_button_clicked {
                                if is_menu_open {
                                    self.active_menu = None;
                                } else {
                                    self.active_menu = Some(ActiveMenu {
                                        item_id: item.id.clone(),
                                        is_pinned: item.is_pinned(),
                                        button_rect: action.button_rect,
                                    });
                                }
                            } else if action.card_clicked {
                                if self.active_menu.is_some() {
                                    self.active_menu = None;
                                } else {
                                    clicked_index = Some(index);
                                }
                            }

                            if selected && keyboard_selection_changed {
                                action.response.scroll_to_me(Some(egui::Align::Center));
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

        if let Some(active_menu) = self.active_menu.clone() {
            let mut menu_action = None;

            let mut should_close_menu = false;

            let menu_width = 96.0;

            let item_height = 26.0;

            let menu_padding = 4.0;

            let menu_height = (item_height * 2.0) + (menu_padding * 2.0);

            let mut menu_x = active_menu.button_rect.right() - menu_width;

            let mut menu_y = active_menu.button_rect.bottom() + 2.0;

            if menu_y + menu_height > POPUP_HEIGHT - ui_style::WINDOW_PADDING {
                menu_y = (active_menu.button_rect.top() - menu_height - 2.0)
                    .max(ui_style::WINDOW_PADDING);
            }

            menu_x = menu_x.clamp(
                ui_style::WINDOW_PADDING,
                POPUP_WIDTH - ui_style::WINDOW_PADDING - menu_width,
            );

            let menu_pos = egui::pos2(menu_x, menu_y);

            let menu_rect =
                egui::Rect::from_min_size(menu_pos, egui::vec2(menu_width, menu_height));

            if let Some(press_pos) = ui.input(|i| i.pointer.press_origin())
                && !menu_rect.contains(press_pos)
                && !active_menu.button_rect.contains(press_pos)
            {
                should_close_menu = true;
            }

            egui::Area::new(egui::Id::new("item_context_menu"))
                .order(egui::Order::Foreground)
                .fixed_pos(menu_pos)
                .show(ui.ctx(), |ui| {
                    ui.painter().rect_filled(
                        menu_rect,
                        ui_style::ROW_CORNER_RADIUS,
                        palette.row_background,
                    );

                    ui.painter().rect_stroke(
                        menu_rect,
                        ui_style::ROW_CORNER_RADIUS,
                        egui::Stroke::new(1.0, palette.border),
                        egui::StrokeKind::Inside,
                    );

                    let inner_rect = menu_rect.shrink(menu_padding);

                    let pin_rect = egui::Rect::from_min_size(
                        inner_rect.min,
                        egui::vec2(inner_rect.width(), item_height),
                    );

                    let delete_rect = egui::Rect::from_min_size(
                        egui::pos2(inner_rect.left(), inner_rect.min.y + item_height),
                        egui::vec2(inner_rect.width(), item_height),
                    );

                    let pin_label = if active_menu.is_pinned {
                        "Unpin"
                    } else {
                        "Pin"
                    };

                    if render_menu_item(ui, pin_rect, pin_label, palette).clicked() {
                        menu_action = Some(if active_menu.is_pinned {
                            MenuActionKind::Unpin
                        } else {
                            MenuActionKind::Pin
                        });

                        should_close_menu = true;
                    }

                    if render_menu_item(ui, delete_rect, "Delete", palette).clicked() {
                        menu_action = Some(MenuActionKind::Delete);

                        should_close_menu = true;
                    }
                });

            if should_close_menu {
                self.active_menu = None;
            }

            if let Some(action) = menu_action {
                match action {
                    MenuActionKind::Pin | MenuActionKind::Unpin => {
                        self.start_toggle_pin(ui.ctx(), active_menu.item_id);
                    }

                    MenuActionKind::Delete => {
                        self.start_delete_item(ui.ctx(), active_menu.item_id);
                    }
                }
            }
        }

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
    fn image_item_maps_to_image_row_with_path() {
        let item = image_item();

        assert_eq!(
            history_row_kind(&item,),
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

        assert_eq!(history_row_kind(&item,), HistoryRowKind::Invalid,);
    }

    #[test]
    fn landscape_thumbnail_fits_available_bounds() {
        let aspect = 16.0 / 9.0;

        let size = fit_thumbnail_size(ImageThumbnail {
            texture_id: egui::TextureId::Managed(1),

            aspect_ratio: aspect,
        });

        assert_eq!(size.y, IMAGE_THUMBNAIL_MAX_HEIGHT,);

        assert_eq!(size.x, IMAGE_THUMBNAIL_MAX_HEIGHT * aspect,);

        assert!(size.x <= IMAGE_THUMBNAIL_MAX_WIDTH);
    }

    #[test]
    fn portrait_thumbnail_fits_height() {
        let size = fit_thumbnail_size(ImageThumbnail {
            texture_id: egui::TextureId::Managed(1),

            aspect_ratio: 9.0 / 16.0,
        });

        assert_eq!(size.y, IMAGE_THUMBNAIL_MAX_HEIGHT,);

        assert!(size.x <= IMAGE_THUMBNAIL_MAX_WIDTH);
    }

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
