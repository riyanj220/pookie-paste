use tokio::sync::mpsc::{self, Receiver};

use wayland_client::{Connection, globals::registry_queue_init, protocol::wl_seat};

use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1;

use crate::{ClipboardEvent, ClipboardWatcher};

use crate::wayland_state::WaylandState;

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

            let (globals, mut event_queue) = registry_queue_init::<WaylandState>(&connection)
                .expect("failed initializing registry");

            let qh = event_queue.handle();

            let manager = globals
                .bind::<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(
                    &qh,
                    1..=2,
                    (),
                )
                .expect("missing zwlr_data_control_manager_v1");

            let seat = globals
                .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())
                .expect("missing wl_seat");

            let device = manager.get_data_device(&seat, &qh, ());

            let mut state = WaylandState {
                device,

                manager,

                seat,

                current_offer: None,

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
