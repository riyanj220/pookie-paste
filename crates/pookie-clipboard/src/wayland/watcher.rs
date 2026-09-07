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
            println!("WAYLAND WATCHER THREAD STARTED");

            let connection = match Connection::connect_to_env() {
                Ok(connection) => connection,

                Err(error) => {
                    eprintln!("FAILED CONNECTING TO WAYLAND {:?}", error);

                    return;
                }
            };

            println!("CONNECTED TO WAYLAND COMPOSITOR");

            let (globals, mut event_queue) =
                match registry_queue_init::<WaylandRegistryState>(&connection) {
                    Ok(value) => value,

                    Err(error) => {
                        eprintln!("FAILED INITIALIZING REGISTRY {:?}", error);

                        return;
                    }
                };

            println!("WAYLAND REGISTRY INITIALIZED");

            let mut registry_state = WaylandRegistryState::default();

            match event_queue.roundtrip(&mut registry_state) {
                Ok(_) => {}

                Err(error) => {
                    eprintln!("REGISTRY ROUNDTRIP FAILED {:?}", error);

                    return;
                }
            }

            registry_state.detect(globals.contents());

            println!(
                "WAYLAND PROTOCOLS ext={} wlr={}",
                registry_state.has_ext_data_control, registry_state.has_wlr_data_control
            );

            if registry_state.has_ext_data_control {
                println!("SELECTING KDE ext_data_control_v1");

                ext_backend::start(connection, globals, sender);

                return;
            }

            if registry_state.has_wlr_data_control {
                println!("SELECTING WLR data_control_v1");

                wlr_backend::start(connection, globals, sender);

                return;
            }

            eprintln!("NO SUPPORTED WAYLAND CLIPBOARD PROTOCOL FOUND");
        });

        receiver
    }
}
