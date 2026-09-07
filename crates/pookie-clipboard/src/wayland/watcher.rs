use tokio::sync::mpsc::{self, Receiver};

use wayland_client::{Connection, globals::registry_queue_init, protocol::wl_seat};

use crate::{ClipboardEvent, ClipboardWatcher};

use super::ext_data_control::ExtDataControlState;
use super::ext_protocol::client::ext_data_control_manager_v1;

pub struct WaylandClipboardWatcher;

impl WaylandClipboardWatcher {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl ClipboardWatcher for WaylandClipboardWatcher {
    fn start(&mut self) -> Receiver<ClipboardEvent> {
        tracing::info!("KDE Wayland watcher start called");

        let (sender, receiver) = mpsc::channel(100);

        std::thread::spawn(move || {
            tracing::info!("KDE Wayland watcher thread started");

            let connection = Connection::connect_to_env().expect("failed connecting to Wayland");

            tracing::info!("Connected to Wayland compositor");

            let (globals, mut event_queue) =
                registry_queue_init::<ExtDataControlState>(&connection)
                    .expect("failed initializing registry");

            tracing::info!("Wayland globals discovered");

            let qh = event_queue.handle();

            tracing::info!("Wayland queue handle created");

            let manager = match globals
                .bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
            {
                Ok(manager) => {
                    tracing::info!("ext_data_control_manager_v1 bound");
                    manager
                }

                Err(error) => {
                    tracing::error!("failed binding ext_data_control_manager_v1: {:?}", error);

                    return;
                }
            };

            let seat = globals
                .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())
                .expect("missing wl_seat");

            let device = manager.get_data_device(&seat, &qh, ());

            tracing::info!("ext_data_control_device_v1 created");

            let mut state = ExtDataControlState {
                manager,

                device,

                seat,

                current_offer: None,

                offers: Vec::new(),

                offered_mime_types: Vec::new(),

                sender,
            };

            tracing::info!("entering KDE Wayland dispatch loop");

            loop {
                if let Err(error) = event_queue.blocking_dispatch(&mut state) {
                    tracing::error!("Wayland dispatch failed: {:?}", error);

                    break;
                }
            }
        });

        receiver
    }
}
