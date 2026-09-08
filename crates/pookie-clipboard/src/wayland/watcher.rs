use tokio::sync::mpsc::{self, Receiver};

use wayland_client::{Connection, globals::registry_queue_init};

use crate::{ClipboardEvent, ClipboardWatcher};

use super::{ext_backend, registry::WaylandRegistryState, wlr_backend};

pub struct WaylandClipboardWatcher;

impl WaylandClipboardWatcher {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl ClipboardWatcher for WaylandClipboardWatcher {
    fn start(&mut self) -> Receiver<ClipboardEvent> {
        let (sender, receiver) = mpsc::channel(100);

        std::thread::spawn(move || {
            tracing::info!("starting wayland clipboard watcher");

            let connection = match Connection::connect_to_env() {
                Ok(connection) => connection,

                Err(error) => {
                    tracing::error!(
                        error = ?error,
                        "failed connecting to wayland compositor"
                    );

                    return;
                }
            };

            tracing::debug!("connected to wayland compositor");

            let (globals, mut event_queue) =
                match registry_queue_init::<WaylandRegistryState>(&connection) {
                    Ok(value) => value,

                    Err(error) => {
                        tracing::error!(
                            error = ?error,
                            "failed initializing wayland registry"
                        );

                        return;
                    }
                };

            let mut registry_state = WaylandRegistryState::default();

            if let Err(error) = event_queue.roundtrip(&mut registry_state) {
                tracing::error!(
                    error = ?error,
                    "wayland registry roundtrip failed"
                );

                return;
            }

            registry_state.detect(globals.contents());

            tracing::debug!(
                ext = registry_state.has_ext_data_control,
                wlr = registry_state.has_wlr_data_control,
                "wayland clipboard protocols detected"
            );

            // EXT data control is currently the preferred implementation.
            if registry_state.has_ext_data_control {
                tracing::info!("selecting ext_data_control_v1 clipboard backend");

                ext_backend::start(connection, globals, sender);

                return;
            }

            if registry_state.has_wlr_data_control {
                tracing::info!("selecting zwlr_data_control_v1 clipboard backend");

                wlr_backend::start(connection, globals, sender);

                return;
            }

            tracing::error!("no supported wayland clipboard protocol found");
        });

        receiver
    }
}
