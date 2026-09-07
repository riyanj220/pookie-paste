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
        let (sender, receiver) = mpsc::channel(100);

        std::thread::spawn(move || {
            let connection = Connection::connect_to_env().expect("failed connecting to Wayland");

            let (globals, mut event_queue) =
                registry_queue_init::<ExtDataControlState>(&connection)
                    .expect("failed initializing registry");

            let qh = event_queue.handle();

            let manager = globals
                .bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
                .expect("missing ext_data_control_manager_v1");

            let seat = globals
                .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())
                .expect("missing wl_seat");

            let device = manager.get_data_device(&seat, &qh, ());

            let mut state = ExtDataControlState {
                manager,

                device,

                seat,

                current_offer: None,

                offers: Vec::new(),

                offered_mime_types: Vec::new(),

                sender,
            };

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
