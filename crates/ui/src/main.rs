mod ipc_client;
mod popup_position;

use eframe::egui;
use tokio::sync::oneshot;

const POPUP_WIDTH: f32 = 360.0;
const POPUP_HEIGHT: f32 = 420.0;
const CURSOR_OFFSET: f32 = 12.0;

const MAX_PREVIEW_LINES: usize = 3;
const MAX_CHARS_PER_LINE: usize = 70;

fn capture_initial_focus_target() -> Option<u64> {
    let runtime = tokio::runtime::Runtime::new().ok()?;

    runtime
        .block_on(ipc_client::capture_focus_target())
        .ok()
        .flatten()
}

fn main() -> eframe::Result<()> {
    let target_id = capture_initial_focus_target();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([POPUP_WIDTH, POPUP_HEIGHT])
        .with_resizable(false)
        .with_decorations(false);

    if let Some([x, y]) = popup_position::cursor_position() {
        viewport = viewport.with_position([x + CURSOR_OFFSET, y + CURSOR_OFFSET]);
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Pookie Paste",
        options,
        Box::new(move |_cc| Ok(Box::new(PookieApp::new(target_id)))),
    )
}

enum HistoryState {
    Loading,
    Loaded(Vec<ipc::HistoryItem>),
    Failed(String),
}

struct PookieApp {
    history: HistoryState,

    history_receiver: Option<oneshot::Receiver<Result<Vec<ipc::HistoryItem>, String>>>,

    selected_index: Option<usize>,

    focus_requested: bool,

    target_id: Option<u64>,

    activation_receiver: Option<oneshot::Receiver<Result<ipc::ActivationOutcome, String>>>,

    activation_in_progress: bool,
}

impl PookieApp {
    fn new(target_id: Option<u64>) -> Self {
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
            focus_requested: false,
            target_id,
            activation_receiver: None,
            activation_in_progress: false,
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

        let target_id = self.target_id;

        let (sender, receiver) = oneshot::channel();

        self.activation_in_progress = true;

        self.activation_receiver = Some(receiver);

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

            Ok(ipc::ActivationOutcome::PasteFailed)
            | Ok(ipc::ActivationOutcome::NotFound)
            | Ok(ipc::ActivationOutcome::UnsupportedContent)
            | Err(_) => {
                self.activation_in_progress = false;

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

impl eframe::App for PookieApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.focus_requested {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);

            self.focus_requested = true;
        }

        self.poll_history();

        self.poll_activation(ui.ctx());

        let move_up = ui.input(|input| input.key_pressed(egui::Key::ArrowUp));

        let move_down = ui.input(|input| input.key_pressed(egui::Key::ArrowDown));

        let activate = ui.input(|input| input.key_pressed(egui::Key::Enter));

        let keyboard_selection_changed = self.handle_keyboard_navigation(move_up, move_down);

        if activate {
            self.start_selected_activation(ui.ctx());
        }

        ui.heading("Clipboard");

        ui.separator();

        let mut clicked_index = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match &self.history {
                HistoryState::Loading => {
                    ui.label("Loading clipboard history...");
                }

                HistoryState::Loaded(items) => {
                    if items.is_empty() {
                        ui.label("No clipboard history yet");

                        return;
                    }

                    for (index, item) in items.iter().enumerate() {
                        let Some(text) = &item.text_content else {
                            continue;
                        };

                        let preview = preview_text(text);

                        let selected = self.selected_index == Some(index);

                        let response = ui.selectable_label(selected, preview);

                        if response.clicked() {
                            clicked_index = Some(index);
                        }

                        if selected && keyboard_selection_changed {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }

                        ui.separator();
                    }
                }

                HistoryState::Failed(error) => {
                    ui.label(format!("Unable to load history: {error}"));
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

    #[test]
    fn short_preview_is_unchanged() {
        assert_eq!(preview_text("Hello world"), "Hello world",);
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

        assert!(preview.ends_with('…'),);
    }
}
