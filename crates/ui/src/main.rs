mod actions;
mod app;
mod controls;
mod header;
mod history;
mod image_thumbnail;
mod ipc_client;
mod platform;
mod popup_anchor;
mod popup_focus;
mod popup_position;
mod rows;
mod shortcut_view;
mod theme;
mod ui_style;

use app::{PookieApp, ViewMode};
use eframe::egui;

pub(crate) const POPUP_WIDTH: f32 = 360.0;
pub(crate) const POPUP_HEIGHT: f32 = 420.0;
const CURSOR_OFFSET: f32 = 12.0;

fn capture_initial_focus_target() -> Option<ipc::IpcFocusTarget> {
    let runtime = tokio::runtime::Runtime::new().ok()?;

    runtime
        .block_on(ipc_client::capture_focus_target())
        .ok()
        .flatten()
}

fn parse_initial_view_mode() -> ViewMode {
    parse_initial_view_mode_from(std::env::args().skip(1))
}

fn parse_initial_view_mode_from(args: impl IntoIterator<Item = impl AsRef<str>>) -> ViewMode {
    for arg in args {
        if arg.as_ref() == "--shortcut-setup" {
            return ViewMode::ShortcutSetup;
        }
    }
    ViewMode::History
}

const APP_ICON_PNG_BYTES: &[u8] = include_bytes!("../assets/pookie-paste-128.png");

fn load_app_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(APP_ICON_PNG_BYTES)
        .ok()?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
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

    let initial_view_mode = parse_initial_view_mode();

    let app_theme = theme::detect_system_theme();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([POPUP_WIDTH, POPUP_HEIGHT])
        .with_resizable(false)
        .with_decorations(false)
        .with_app_id("io.github.riyanj220.PookiePaste");

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    if let Some(position) = platform::resolve_popup_position(
        target_id.as_ref(),
        POPUP_WIDTH,
        POPUP_HEIGHT,
        CURSOR_OFFSET,
    ) {
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

            Ok(Box::new(PookieApp::new(
                target_id,
                initial_view_mode,
                cc.egui_ctx.clone(),
            )))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ui_startup_mode_remains_history() {
        assert_eq!(
            parse_initial_view_mode_from(Vec::<String>::new()),
            ViewMode::History
        );
        assert_eq!(
            parse_initial_view_mode_from(["--random-arg"]),
            ViewMode::History
        );
    }

    #[test]
    fn shortcut_setup_startup_mode_resolves_to_shortcut_setup() {
        assert_eq!(
            parse_initial_view_mode_from(["--shortcut-setup"]),
            ViewMode::ShortcutSetup
        );
        assert_eq!(
            parse_initial_view_mode_from(["--other", "--shortcut-setup"]),
            ViewMode::ShortcutSetup
        );
    }
}
