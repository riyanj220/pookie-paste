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

use app::PookieApp;
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

            Ok(Box::new(PookieApp::new(target_id, cc.egui_ctx.clone())))
        }),
    )
}
