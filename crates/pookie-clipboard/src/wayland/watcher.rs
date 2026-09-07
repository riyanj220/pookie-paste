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
        println!("WAYLAND WATCHER START CALLED");

        tracing::info!("KDE Wayland watcher start called");

        let (sender, receiver) = mpsc::channel(100);

        std::thread::spawn(move || {
            println!("WAYLAND WATCHER THREAD STARTED");

            let connection = Connection::connect_to_env().expect("failed connecting to Wayland");

            println!("CONNECTED TO WAYLAND COMPOSITOR");

            let (globals, mut event_queue) =
                registry_queue_init::<ExtDataControlState>(&connection)
                    .expect("failed initializing registry");

            println!("WAYLAND GLOBALS DISCOVERED");

            let qh = event_queue.handle();

            println!("WAYLAND QUEUE HANDLE CREATED");

            let manager = globals
                .bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
                .expect("missing ext_data_control_manager_v1");

            println!("EXT DATA CONTROL MANAGER BOUND");

            let seat = globals
                .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())
                .expect("missing wl_seat");

            println!("WL SEAT BOUND");

            let device = manager.get_data_device(&seat, &qh, ());

            println!("EXT DATA CONTROL DEVICE CREATED");

            let mut state = ExtDataControlState {
                manager,

                device,

                seat,

                current_offer: None,

                offers: Vec::new(),

                offered_mime_types: Vec::new(),

                sender,
            };

            println!("STATE CREATED");

            connection.roundtrip().expect("wayland roundtrip failed");

            println!("WAYLAND ROUNDTRIP COMPLETE");

            println!("ENTERING WAYLAND DISPATCH LOOP");

            loop {
                match event_queue.blocking_dispatch(&mut state) {
                    Ok(_) => {
                        tracing::info!("Wayland dispatch cycle completed");
                    }

                    Err(error) => {
                        println!("WAYLAND DISPATCH FAILED: {:?}", error);

                        tracing::error!("Wayland dispatch failed: {:?}", error);

                        break;
                    }
                }
            }
        });

        receiver
    }
}
