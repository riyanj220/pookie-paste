use eframe::egui;
use ipc::{
    IpcCompositorBindingStatus, IpcShortcutCapability, IpcShortcutState, ShortcutStatusInfo,
};

use crate::ui_style::{self, UiPalette};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCandidate {
    pub modifiers: Vec<String>,
    pub key: String,
}

impl KeyCandidate {
    pub fn display_string(&self) -> String {
        let mut parts = Vec::new();
        for m in &self.modifiers {
            match m.to_uppercase().as_str() {
                "SUPER" => parts.push("Super".to_string()),
                "CTRL" | "CONTROL" => parts.push("Ctrl".to_string()),
                "ALT" => parts.push("Alt".to_string()),
                "SHIFT" => parts.push("Shift".to_string()),
                other => parts.push(other.to_string()),
            }
        }
        parts.push(self.key.clone());
        parts.join("+")
    }
}

#[derive(Debug, Default, Clone)]
pub struct ShortcutViewState {
    pub is_recording: bool,
    pub candidate: Option<KeyCandidate>,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub copied_feedback: bool,
}

pub enum ShortcutViewAction {
    None,
    SwitchToHistory,
    SaveShortcut { modifiers: Vec<String>, key: String },
    ConfigurePortal,
    RecheckStatus,
    CopySnippet(String),
}

/// Renders a non-blocking attention banner above the clipboard history
/// if shortcut configuration requires attention (e.g. Unconfigured or Conflict).
pub fn render_attention_banner(
    ui: &mut egui::Ui,
    status: Option<&ShortcutStatusInfo>,
    palette: UiPalette,
) -> bool {
    let Some(status) = status else {
        return false;
    };

    let banner_text = match &status.state {
        IpcShortcutState::CompositorManaged {
            binding_status: IpcCompositorBindingStatus::Unconfigured,
            ..
        } => Some("⚠️ Shortcut not bound in compositor"),
        IpcShortcutState::CompositorManaged {
            binding_status: IpcCompositorBindingStatus::Conflict,
            ..
        } => Some("⚠️ Shortcut conflict detected in compositor"),
        IpcShortcutState::Unavailable { .. } => Some("⚠️ Global shortcut is currently unavailable"),
        _ => None,
    };

    let Some(text) = banner_text else {
        return false;
    };

    let mut clicked = false;
    let bg_color = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(60, 45, 15)
    } else {
        egui::Color32::from_rgb(254, 243, 199)
    };
    let border_color = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(180, 130, 40)
    } else {
        egui::Color32::from_rgb(245, 158, 11)
    };
    let text_color = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(250, 220, 150)
    } else {
        egui::Color32::from_rgb(146, 64, 14)
    };

    egui::Frame::new()
        .fill(bg_color)
        .stroke(egui::Stroke::new(1.0, border_color))
        .corner_radius(ui_style::ROW_CORNER_RADIUS)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(ui_style::BODY_TEXT_SIZE - 1.0)
                        .color(text_color)
                        .strong(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let config_btn = egui::Button::new(
                        egui::RichText::new("Configure")
                            .size(ui_style::BODY_TEXT_SIZE - 2.0)
                            .color(palette.text_primary),
                    );
                    if ui.add(config_btn).clicked() {
                        clicked = true;
                    }
                });
            });
        });

    ui.add_space(4.0);
    clicked
}

/// Renders the full shortcut setup view inside the popup.
pub fn render_shortcut_setup(
    ui: &mut egui::Ui,
    state: &mut ShortcutViewState,
    status: Option<&ShortcutStatusInfo>,
    palette: UiPalette,
) -> ShortcutViewAction {
    let mut action = ShortcutViewAction::None;

    ui.add_space(ui_style::WINDOW_PADDING);

    // Navigation row: Back button
    ui.horizontal(|ui| {
        let back_btn = egui::Button::new(
            egui::RichText::new("← Back to History")
                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                .color(palette.accent),
        )
        .frame(false);

        if ui
            .add(back_btn)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            action = ShortcutViewAction::SwitchToHistory;
        }
    });

    ui.add_space(8.0);

    let Some(status) = status else {
        ui.label(
            egui::RichText::new("Connecting to daemon...")
                .size(ui_style::BODY_TEXT_SIZE)
                .color(palette.text_secondary),
        );
        return action;
    };

    // Card: Current Status & Backend
    egui::Frame::new()
        .fill(palette.row_background)
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(ui_style::ROW_CORNER_RADIUS)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Backend:")
                        .size(ui_style::BODY_TEXT_SIZE - 1.0)
                        .color(palette.text_secondary),
                );

                let backend_label = match (status.capability, status.backend_name.as_deref()) {
                    (Some(IpcShortcutCapability::Native), Some(name)) => name.to_string(),
                    (Some(IpcShortcutCapability::Native), None) => "Native (X11)".to_string(),
                    (Some(IpcShortcutCapability::CompositorManaged), Some(name)) => {
                        format!("Compositor ({name})")
                    }
                    (Some(IpcShortcutCapability::CompositorManaged), None) => {
                        "Compositor Managed".to_string()
                    }
                    (Some(IpcShortcutCapability::Portal), _) => "Desktop Portal (KDE)".to_string(),
                    (Some(IpcShortcutCapability::Unsupported), _) => "Unsupported".to_string(),
                    (None, _) => "Detecting...".to_string(),
                };

                ui.label(
                    egui::RichText::new(backend_label)
                        .size(ui_style::BODY_TEXT_SIZE - 1.0)
                        .color(palette.text_primary)
                        .strong(),
                );
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Status:")
                        .size(ui_style::BODY_TEXT_SIZE - 1.0)
                        .color(palette.text_secondary),
                );

                let (status_text, status_color) = match &status.state {
                    IpcShortcutState::Active { .. } => (
                        "Active",
                        if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(74, 222, 128)
                        } else {
                            egui::Color32::from_rgb(22, 163, 74)
                        },
                    ),
                    IpcShortcutState::CompositorManaged {
                        binding_status: IpcCompositorBindingStatus::Verified,
                        ..
                    } => (
                        "Active (Verified)",
                        if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(74, 222, 128)
                        } else {
                            egui::Color32::from_rgb(22, 163, 74)
                        },
                    ),
                    IpcShortcutState::CompositorManaged {
                        binding_status: IpcCompositorBindingStatus::BoundUnverified,
                        ..
                    } => ("Bound (Unverified)", palette.accent),
                    IpcShortcutState::CompositorManaged {
                        binding_status: IpcCompositorBindingStatus::Unconfigured,
                        ..
                    } => (
                        "Setup required",
                        if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(251, 191, 36)
                        } else {
                            egui::Color32::from_rgb(217, 119, 6)
                        },
                    ),
                    IpcShortcutState::CompositorManaged {
                        binding_status: IpcCompositorBindingStatus::Conflict,
                        ..
                    } => (
                        "Shortcut conflict",
                        if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(248, 113, 113)
                        } else {
                            egui::Color32::from_rgb(220, 38, 38)
                        },
                    ),
                    IpcShortcutState::Initializing => ("Initializing...", palette.text_secondary),
                    IpcShortcutState::Conflict { .. } => (
                        "Shortcut conflict",
                        if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(248, 113, 113)
                        } else {
                            egui::Color32::from_rgb(220, 38, 38)
                        },
                    ),
                    IpcShortcutState::Failed { .. } => (
                        "Failed",
                        if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(248, 113, 113)
                        } else {
                            egui::Color32::from_rgb(220, 38, 38)
                        },
                    ),
                    IpcShortcutState::Unavailable { .. } => (
                        "Unavailable",
                        if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(248, 113, 113)
                        } else {
                            egui::Color32::from_rgb(220, 38, 38)
                        },
                    ),
                };

                ui.label(
                    egui::RichText::new(status_text)
                        .size(ui_style::BODY_TEXT_SIZE - 1.0)
                        .color(status_color)
                        .strong(),
                );
            });

            if let Some(effective) = &status.effective_shortcut
                && effective != &status.configured_shortcut
            {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Portal effective:")
                            .size(ui_style::BODY_TEXT_SIZE - 2.0)
                            .color(palette.text_secondary),
                    );
                    ui.label(
                        egui::RichText::new(effective)
                            .size(ui_style::BODY_TEXT_SIZE - 2.0)
                            .color(palette.accent)
                            .strong(),
                    );
                });
            }
        });

    ui.add_space(10.0);

    // Main interaction section based on capability
    match status.capability {
        Some(IpcShortcutCapability::Native) | Some(IpcShortcutCapability::CompositorManaged) => {
            render_key_recorder_section(ui, state, status, palette, &mut action);

            // If CompositorManaged, verbatim render the directive snippet card
            if let IpcShortcutState::CompositorManaged {
                snippet,
                conflict,
                diagnostic,
                ..
            } = &status.state
            {
                ui.add_space(10.0);
                render_compositor_snippet_card(
                    ui,
                    state,
                    snippet,
                    conflict.as_deref(),
                    diagnostic.as_deref(),
                    palette,
                    &mut action,
                );
            }
        }

        Some(IpcShortcutCapability::Portal) => {
            render_portal_section(ui, status, palette, &mut action);
        }

        Some(IpcShortcutCapability::Unsupported) | None => {
            ui.label(
                egui::RichText::new(
                    "Global shortcuts are not supported in this desktop environment.",
                )
                .size(ui_style::BODY_TEXT_SIZE)
                .color(palette.text_secondary),
            );
        }
    }

    // Feedback messages
    if let Some(err) = &state.error_message {
        ui.add_space(8.0);
        let err_color = if ui.visuals().dark_mode {
            egui::Color32::from_rgb(248, 113, 113)
        } else {
            egui::Color32::from_rgb(220, 38, 38)
        };
        ui.label(
            egui::RichText::new(format!("⚠️ {err}"))
                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                .color(err_color),
        );
    }

    if let Some(ok) = &state.success_message {
        ui.add_space(8.0);
        let ok_color = if ui.visuals().dark_mode {
            egui::Color32::from_rgb(74, 222, 128)
        } else {
            egui::Color32::from_rgb(22, 163, 74)
        };
        ui.label(
            egui::RichText::new(format!("✓ {ok}"))
                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                .color(ok_color),
        );
    }

    action
}

fn render_key_recorder_section(
    ui: &mut egui::Ui,
    state: &mut ShortcutViewState,
    status: &ShortcutStatusInfo,
    palette: UiPalette,
    action: &mut ShortcutViewAction,
) {
    ui.label(
        egui::RichText::new("Shortcut Keybinding")
            .size(ui_style::BODY_TEXT_SIZE)
            .color(palette.text_primary)
            .strong(),
    );

    ui.add_space(4.0);

    if state.is_recording {
        // Active recording UI
        let border_color = palette.accent;
        egui::Frame::new()
            .fill(palette.row_selected)
            .stroke(egui::Stroke::new(1.5, border_color))
            .corner_radius(ui_style::ROW_CORNER_RADIUS)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Press your shortcut key combination…")
                            .size(ui_style::BODY_TEXT_SIZE)
                            .color(palette.accent)
                            .strong(),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(
                            "(Hold Super, Ctrl, Alt, or Shift + Key. Press Esc to cancel)",
                        )
                        .size(ui_style::BODY_TEXT_SIZE - 2.0)
                        .color(palette.text_secondary),
                    );
                });
            });

        // Input capture loop for local-only key recording
        ui.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                state.is_recording = false;
                state.candidate = None;
                return;
            }

            let active_modifiers =
                extract_active_modifiers(&input.modifiers, |k| input.key_down(k));

            if !active_modifiers.is_empty() {
                let outcome = process_recording_events(&input.events, &active_modifiers);
                match outcome {
                    RecordingEventOutcome::Captured { modifiers, key } => {
                        tracing::debug!(
                            ?modifiers,
                            %key,
                            "captured shortcut key candidate via event stream"
                        );
                        state.candidate = Some(KeyCandidate { modifiers, key });
                        state.is_recording = false;
                        state.error_message = None;
                    }
                    RecordingEventOutcome::Cancel => {
                        state.is_recording = false;
                        state.candidate = None;
                    }
                    RecordingEventOutcome::None => {
                        // Fallback for cases where logical key_pressed was registered
                        if let Some(key_str) = capture_pressed_key(input) {
                            tracing::debug!(
                                ?active_modifiers,
                                %key_str,
                                "captured shortcut key candidate via key_pressed fallback"
                            );
                            state.candidate = Some(KeyCandidate {
                                modifiers: active_modifiers,
                                key: key_str,
                            });
                            state.is_recording = false;
                            state.error_message = None;
                        }
                    }
                }
            }
        });

        ui.add_space(6.0);
        if ui
            .button(
                egui::RichText::new("Cancel Recording")
                    .size(ui_style::BODY_TEXT_SIZE - 1.0)
                    .color(palette.text_secondary),
            )
            .clicked()
        {
            state.is_recording = false;
            state.candidate = None;
        }
    } else if let Some(candidate) = state.candidate.clone() {
        // Candidate recorded, waiting for explicit save
        let display = candidate.display_string();
        egui::Frame::new()
            .fill(palette.row_background)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(ui_style::ROW_CORNER_RADIUS)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("New Shortcut:")
                            .size(ui_style::BODY_TEXT_SIZE)
                            .color(palette.text_secondary),
                    );
                    ui.label(
                        egui::RichText::new(&display)
                            .size(ui_style::BODY_TEXT_SIZE + 1.0)
                            .color(palette.accent)
                            .strong(),
                    );
                });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let save_btn = egui::Button::new(
                        egui::RichText::new("Save Shortcut")
                            .size(ui_style::BODY_TEXT_SIZE - 1.0)
                            .color(palette.text_primary),
                    );
                    if ui.add(save_btn).clicked() {
                        *action = ShortcutViewAction::SaveShortcut {
                            modifiers: candidate.modifiers.clone(),
                            key: candidate.key.clone(),
                        };
                    }

                    let cancel_btn = egui::Button::new(
                        egui::RichText::new("Discard")
                            .size(ui_style::BODY_TEXT_SIZE - 1.0)
                            .color(palette.text_secondary),
                    );
                    if ui.add(cancel_btn).clicked() {
                        state.candidate = None;
                        state.error_message = None;
                    }
                });
            });
    } else {
        // Idle display of current shortcut
        egui::Frame::new()
            .fill(palette.row_background)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(ui_style::ROW_CORNER_RADIUS)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Configured:")
                            .size(ui_style::BODY_TEXT_SIZE)
                            .color(palette.text_secondary),
                    );
                    ui.label(
                        egui::RichText::new(&status.configured_shortcut)
                            .size(ui_style::BODY_TEXT_SIZE)
                            .color(palette.text_primary)
                            .strong(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let change_btn = egui::Button::new(
                            egui::RichText::new("Change…")
                                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                                .color(palette.accent),
                        );
                        if ui.add(change_btn).clicked() {
                            state.is_recording = true;
                            state.candidate = None;
                            state.error_message = None;
                            state.success_message = None;
                        }
                    });
                });
            });
    }
}

fn render_compositor_snippet_card(
    ui: &mut egui::Ui,
    state: &mut ShortcutViewState,
    snippet: &str,
    conflict: Option<&str>,
    diagnostic: Option<&str>,
    palette: UiPalette,
    action: &mut ShortcutViewAction,
) {
    ui.label(
        egui::RichText::new("Compositor Configuration Directive")
            .size(ui_style::BODY_TEXT_SIZE)
            .color(palette.text_primary)
            .strong(),
    );

    ui.add_space(2.0);
    ui.label(
        egui::RichText::new("Add this line to your compositor configuration file:")
            .size(ui_style::BODY_TEXT_SIZE - 2.0)
            .color(palette.text_secondary),
    );

    ui.add_space(4.0);

    // Monospace directive snippet
    egui::Frame::new()
        .fill(if ui.visuals().dark_mode {
            egui::Color32::from_rgb(18, 18, 18)
        } else {
            egui::Color32::from_rgb(238, 238, 238)
        })
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(ui_style::ROW_CORNER_RADIUS)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(snippet)
                        .monospace()
                        .size(ui_style::BODY_TEXT_SIZE - 1.0)
                        .color(palette.text_primary),
                )
                .wrap(),
            );
        });

    ui.add_space(6.0);

    ui.horizontal(|ui| {
        let copy_label = if state.copied_feedback {
            "✓ Copied!"
        } else {
            "Copy Directive"
        };
        let copy_btn = egui::Button::new(
            egui::RichText::new(copy_label)
                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                .color(palette.text_primary),
        );
        if ui.add(copy_btn).clicked() {
            ui.ctx().copy_text(snippet.to_string());
            state.copied_feedback = true;
            *action = ShortcutViewAction::CopySnippet(snippet.to_string());
        }

        ui.add_space(6.0);

        let check_btn = egui::Button::new(
            egui::RichText::new("Check Again")
                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                .color(palette.accent),
        );
        if ui.add(check_btn).clicked() {
            *action = ShortcutViewAction::RecheckStatus;
        }
    });

    if let Some(conf) = conflict {
        ui.add_space(4.0);
        let warn_color = if ui.visuals().dark_mode {
            egui::Color32::from_rgb(251, 191, 36)
        } else {
            egui::Color32::from_rgb(217, 119, 6)
        };
        ui.label(
            egui::RichText::new(format!("Notice: {conf}"))
                .size(ui_style::BODY_TEXT_SIZE - 2.0)
                .color(warn_color),
        );
    } else if let Some(diag) = diagnostic {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(diag)
                .size(ui_style::BODY_TEXT_SIZE - 2.0)
                .color(palette.text_secondary),
        );
    }
}

fn render_portal_section(
    ui: &mut egui::Ui,
    status: &ShortcutStatusInfo,
    palette: UiPalette,
    action: &mut ShortcutViewAction,
) {
    ui.label(
        egui::RichText::new("KDE Plasma Desktop Portal")
            .size(ui_style::BODY_TEXT_SIZE)
            .color(palette.text_primary)
            .strong(),
    );

    ui.add_space(4.0);

    egui::Frame::new()
        .fill(palette.row_background)
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(ui_style::ROW_CORNER_RADIUS)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "Global shortcuts in KDE Plasma are managed securely by the desktop portal. Configure the shortcut in the native KDE dialog below:",
                )
                .size(ui_style::BODY_TEXT_SIZE - 1.0)
                .color(palette.text_secondary),
            );

            ui.add_space(8.0);

            let portal_btn = egui::Button::new(
                egui::RichText::new("Configure in KDE Settings")
                    .size(ui_style::BODY_TEXT_SIZE)
                    .color(palette.accent)
                    .strong(),
            );
            if ui.add(portal_btn).clicked() {
                *action = ShortcutViewAction::ConfigurePortal;
            }
        });

    if let Some(effective) = &status.effective_shortcut {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Currently effective in KDE:")
                    .size(ui_style::BODY_TEXT_SIZE - 1.0)
                    .color(palette.text_secondary),
            );
            ui.label(
                egui::RichText::new(effective)
                    .size(ui_style::BODY_TEXT_SIZE - 1.0)
                    .color(palette.text_primary)
                    .strong(),
            );
        });
    }
}

/// Extracts active modifiers from egui's InputState.
/// Checks both egui's collapsed Modifiers and physical modifier keys in `keys_down`.
/// This ensures Linux Super (SuperLeft/SuperRight) is robustly detected despite egui-winit's
/// macOS-only gate on `mac_cmd`.
pub fn extract_active_modifiers(
    modifiers: &egui::Modifiers,
    is_key_down: impl Fn(egui::Key) -> bool,
) -> Vec<String> {
    let mut mods = Vec::new();

    let super_pressed = modifiers.mac_cmd
        || is_key_down(egui::Key::SuperLeft)
        || is_key_down(egui::Key::SuperRight);
    if super_pressed {
        mods.push("SUPER".to_string());
    }

    let ctrl_pressed = modifiers.ctrl
        || is_key_down(egui::Key::ControlLeft)
        || is_key_down(egui::Key::ControlRight);
    if ctrl_pressed {
        mods.push("CTRL".to_string());
    }

    let alt_pressed =
        modifiers.alt || is_key_down(egui::Key::AltLeft) || is_key_down(egui::Key::AltRight);
    if alt_pressed {
        mods.push("ALT".to_string());
    }

    let shift_pressed =
        modifiers.shift || is_key_down(egui::Key::ShiftLeft) || is_key_down(egui::Key::ShiftRight);
    if shift_pressed {
        mods.push("SHIFT".to_string());
    }

    mods
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingEventOutcome {
    None,
    Cancel,
    Captured { modifiers: Vec<String>, key: String },
}

/// Inspects egui's event stream during shortcut recording.
/// Handles:
/// - Key presses for supported keys
/// - Bare modifier rejection (Super, Ctrl, Alt, Shift alone)
/// - Escape cancellation
/// - Semantic clipboard events produced by egui-winit (Paste -> "V", Copy -> "C", Cut -> "X")
pub fn process_recording_events<'a>(
    events: impl IntoIterator<Item = &'a egui::Event>,
    active_modifiers: &[String],
) -> RecordingEventOutcome {
    if active_modifiers.is_empty() {
        return RecordingEventOutcome::None;
    }

    for event in events {
        match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } => {
                if *key == egui::Key::Escape {
                    return RecordingEventOutcome::Cancel;
                }
                if is_modifier_key(*key) {
                    continue;
                }
                if let Some(key_name) = key_to_shortcut_str(*key) {
                    return RecordingEventOutcome::Captured {
                        modifiers: active_modifiers.to_vec(),
                        key: key_name.to_string(),
                    };
                }
            }
            egui::Event::Paste(_) => {
                return RecordingEventOutcome::Captured {
                    modifiers: active_modifiers.to_vec(),
                    key: "V".to_string(),
                };
            }
            egui::Event::Copy => {
                return RecordingEventOutcome::Captured {
                    modifiers: active_modifiers.to_vec(),
                    key: "C".to_string(),
                };
            }
            egui::Event::Cut => {
                return RecordingEventOutcome::Captured {
                    modifiers: active_modifiers.to_vec(),
                    key: "X".to_string(),
                };
            }
            _ => {}
        }
    }

    RecordingEventOutcome::None
}

pub fn is_modifier_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::SuperLeft
            | egui::Key::SuperRight
            | egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::ShiftLeft
            | egui::Key::ShiftRight
    )
}

pub fn key_to_shortcut_str(key: egui::Key) -> Option<&'static str> {
    match key {
        egui::Key::A => Some("A"),
        egui::Key::B => Some("B"),
        egui::Key::C => Some("C"),
        egui::Key::D => Some("D"),
        egui::Key::E => Some("E"),
        egui::Key::F => Some("F"),
        egui::Key::G => Some("G"),
        egui::Key::H => Some("H"),
        egui::Key::I => Some("I"),
        egui::Key::J => Some("J"),
        egui::Key::K => Some("K"),
        egui::Key::L => Some("L"),
        egui::Key::M => Some("M"),
        egui::Key::N => Some("N"),
        egui::Key::O => Some("O"),
        egui::Key::P => Some("P"),
        egui::Key::Q => Some("Q"),
        egui::Key::R => Some("R"),
        egui::Key::S => Some("S"),
        egui::Key::T => Some("T"),
        egui::Key::U => Some("U"),
        egui::Key::V => Some("V"),
        egui::Key::W => Some("W"),
        egui::Key::X => Some("X"),
        egui::Key::Y => Some("Y"),
        egui::Key::Z => Some("Z"),
        egui::Key::Num0 => Some("0"),
        egui::Key::Num1 => Some("1"),
        egui::Key::Num2 => Some("2"),
        egui::Key::Num3 => Some("3"),
        egui::Key::Num4 => Some("4"),
        egui::Key::Num5 => Some("5"),
        egui::Key::Num6 => Some("6"),
        egui::Key::Num7 => Some("7"),
        egui::Key::Num8 => Some("8"),
        egui::Key::Num9 => Some("9"),
        egui::Key::Space => Some("Space"),
        egui::Key::Tab => Some("Tab"),
        egui::Key::Enter => Some("Enter"),
        egui::Key::Insert => Some("Insert"),
        egui::Key::Delete => Some("Delete"),
        egui::Key::F1 => Some("F1"),
        egui::Key::F2 => Some("F2"),
        egui::Key::F3 => Some("F3"),
        egui::Key::F4 => Some("F4"),
        egui::Key::F5 => Some("F5"),
        egui::Key::F6 => Some("F6"),
        egui::Key::F7 => Some("F7"),
        egui::Key::F8 => Some("F8"),
        egui::Key::F9 => Some("F9"),
        egui::Key::F10 => Some("F10"),
        egui::Key::F11 => Some("F11"),
        egui::Key::F12 => Some("F12"),
        _ => None,
    }
}

fn capture_pressed_key(input: &egui::InputState) -> Option<String> {
    const ALL_KEYS: &[egui::Key] = &[
        egui::Key::A,
        egui::Key::B,
        egui::Key::C,
        egui::Key::D,
        egui::Key::E,
        egui::Key::F,
        egui::Key::G,
        egui::Key::H,
        egui::Key::I,
        egui::Key::J,
        egui::Key::K,
        egui::Key::L,
        egui::Key::M,
        egui::Key::N,
        egui::Key::O,
        egui::Key::P,
        egui::Key::Q,
        egui::Key::R,
        egui::Key::S,
        egui::Key::T,
        egui::Key::U,
        egui::Key::V,
        egui::Key::W,
        egui::Key::X,
        egui::Key::Y,
        egui::Key::Z,
        egui::Key::Num0,
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
        egui::Key::Num6,
        egui::Key::Num7,
        egui::Key::Num8,
        egui::Key::Num9,
        egui::Key::Space,
        egui::Key::Tab,
        egui::Key::Enter,
        egui::Key::Insert,
        egui::Key::Delete,
        egui::Key::F1,
        egui::Key::F2,
        egui::Key::F3,
        egui::Key::F4,
        egui::Key::F5,
        egui::Key::F6,
        egui::Key::F7,
        egui::Key::F8,
        egui::Key::F9,
        egui::Key::F10,
        egui::Key::F11,
        egui::Key::F12,
    ];

    for &key in ALL_KEYS {
        if input.key_pressed(key)
            && let Some(name) = key_to_shortcut_str(key)
        {
            return Some(name.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_candidate_formats_display_string() {
        let cand = KeyCandidate {
            modifiers: vec!["CTRL".to_string(), "SHIFT".to_string()],
            key: "P".to_string(),
        };
        assert_eq!(cand.display_string(), "Ctrl+Shift+P");

        let cand2 = KeyCandidate {
            modifiers: vec!["SUPER".to_string()],
            key: "V".to_string(),
        };
        assert_eq!(cand2.display_string(), "Super+V");
    }

    #[test]
    fn extract_active_modifiers_detects_linux_super_via_physical_keys() {
        let modifiers = egui::Modifiers::default();
        // On Linux, mac_cmd is false
        assert!(!modifiers.mac_cmd);

        // When SuperLeft is down
        let mods_left = extract_active_modifiers(&modifiers, |k| k == egui::Key::SuperLeft);
        assert_eq!(mods_left, vec!["SUPER".to_string()]);

        // When SuperRight is down
        let mods_right = extract_active_modifiers(&modifiers, |k| k == egui::Key::SuperRight);
        assert_eq!(mods_right, vec!["SUPER".to_string()]);
    }

    #[test]
    fn extract_active_modifiers_combines_multiple_modifiers() {
        let modifiers = egui::Modifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        };

        let mods = extract_active_modifiers(&modifiers, |k| {
            k == egui::Key::SuperLeft || k == egui::Key::ShiftRight
        });
        assert_eq!(
            mods,
            vec![
                "SUPER".to_string(),
                "CTRL".to_string(),
                "ALT".to_string(),
                "SHIFT".to_string()
            ]
        );
    }

    #[test]
    fn process_recording_events_captures_key_with_modifiers() {
        let active_mods = vec!["SUPER".to_string()];
        let events = vec![egui::Event::Key {
            key: egui::Key::K,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }];

        let outcome = process_recording_events(&events, &active_mods);
        assert_eq!(
            outcome,
            RecordingEventOutcome::Captured {
                modifiers: vec!["SUPER".to_string()],
                key: "K".to_string(),
            }
        );
    }

    #[test]
    fn process_recording_events_captures_semantic_paste_event_as_v() {
        let active_mods = vec!["CTRL".to_string()];
        let events = vec![egui::Event::Paste("clipboard content".to_string())];

        let outcome = process_recording_events(&events, &active_mods);
        assert_eq!(
            outcome,
            RecordingEventOutcome::Captured {
                modifiers: vec!["CTRL".to_string()],
                key: "V".to_string(),
            }
        );
    }

    #[test]
    fn process_recording_events_captures_semantic_copy_event_as_c() {
        let active_mods = vec!["CTRL".to_string()];
        let events = vec![egui::Event::Copy];

        let outcome = process_recording_events(&events, &active_mods);
        assert_eq!(
            outcome,
            RecordingEventOutcome::Captured {
                modifiers: vec!["CTRL".to_string()],
                key: "C".to_string(),
            }
        );
    }

    #[test]
    fn process_recording_events_captures_semantic_cut_event_as_x() {
        let active_mods = vec!["CTRL".to_string()];
        let events = vec![egui::Event::Cut];

        let outcome = process_recording_events(&events, &active_mods);
        assert_eq!(
            outcome,
            RecordingEventOutcome::Captured {
                modifiers: vec!["CTRL".to_string()],
                key: "X".to_string(),
            }
        );
    }

    #[test]
    fn process_recording_events_ignores_modifier_only_presses() {
        let active_mods = vec!["SUPER".to_string()];
        let events = vec![egui::Event::Key {
            key: egui::Key::SuperLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }];

        let outcome = process_recording_events(&events, &active_mods);
        assert_eq!(outcome, RecordingEventOutcome::None);
    }

    #[test]
    fn process_recording_events_cancels_on_escape() {
        let active_mods = vec!["CTRL".to_string()];
        let events = vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }];

        let outcome = process_recording_events(&events, &active_mods);
        assert_eq!(outcome, RecordingEventOutcome::Cancel);
    }

    #[test]
    fn process_recording_events_ignores_when_no_modifiers() {
        let active_mods: Vec<String> = Vec::new();
        let events = vec![egui::Event::Key {
            key: egui::Key::K,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }];

        let outcome = process_recording_events(&events, &active_mods);
        assert_eq!(outcome, RecordingEventOutcome::None);
    }
}
