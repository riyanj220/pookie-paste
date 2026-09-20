use std::time::{Duration, Instant};

use eframe::egui;
use tokio::sync::oneshot;

use crate::actions::{ActiveMenu, MenuActionKind, UiActionOutcome};
use crate::controls::render_menu_item;
use crate::header::render_header;
use crate::history::HistoryState;
use crate::image_thumbnail::ImageThumbnailCache;
use crate::ipc_client;
use crate::popup_focus::{self, FocusRequestState};
use crate::rows::{
    RowVisualState, render_history_item_row, render_state_message, render_status_message,
};
use crate::theme::AppTheme;
use crate::ui_style;
use crate::{POPUP_HEIGHT, POPUP_WIDTH};

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

pub(crate) struct PookieApp {
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
    pub(crate) fn new(
        target_id: Option<ipc::IpcFocusTarget>,
        repaint_context: egui::Context,
    ) -> Self {
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
