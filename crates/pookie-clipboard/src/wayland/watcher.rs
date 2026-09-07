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

        let (sender, receiver) = mpsc::channel(100);

        std::thread::spawn(move || {
            println!("WAYLAND WATCHER THREAD STARTED");

            let connection = match Connection::connect_to_env() {
                Ok(connection) => connection,

                Err(error) => {
                    eprintln!("FAILED CONNECTING TO WAYLAND: {:?}", error);

                    return;
                }
            };

            println!("CONNECTED TO WAYLAND COMPOSITOR");

            let (globals, mut event_queue) =
                match registry_queue_init::<ExtDataControlState>(&connection) {
                    Ok(value) => value,

                    Err(error) => {
                        eprintln!("FAILED INITIALIZING WAYLAND REGISTRY: {:?}", error);

                        return;
                    }
                };

            println!("WAYLAND GLOBALS DISCOVERED");

            let qh = event_queue.handle();

            println!("WAYLAND QUEUE HANDLE CREATED");

            let manager = match globals
                .bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
            {
                Ok(manager) => manager,

                Err(error) => {
                    eprintln!("FAILED BINDING EXT DATA CONTROL MANAGER {:?}", error);

                    return;
                }
            };

            println!("EXT DATA CONTROL MANAGER BOUND");

            let seat = match globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ()) {
                Ok(seat) => seat,

                Err(error) => {
                    eprintln!("FAILED BINDING WL_SEAT {:?}", error);

                    return;
                }
            };

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

                clipboard_requested: false,
            };

            println!("STATE CREATED");

            /*
             * Force KDE to send initial clipboard state.
             */
            match connection.roundtrip() {
                Ok(_) => {
                    println!("WAYLAND ROUNDTRIP COMPLETE");
                }

                Err(error) => {
                    eprintln!("WAYLAND ROUNDTRIP FAILED {:?}", error);

                    return;
                }
            }

            let mut event_counter = 0u64;

            println!("ENTERING WAYLAND DISPATCH LOOP");

            loop {
                if let Err(error) = connection.flush() {
                    eprintln!("WAYLAND FLUSH FAILED {:?}", error);
                }

                tracing::info!("waiting for Wayland events");

                match event_queue.blocking_dispatch(&mut state) {
                    Ok(dispatched) => {
                        event_counter += dispatched as u64;

                        println!(
                            "WAYLAND DISPATCH COMPLETE events={} total={}",
                            dispatched, event_counter
                        );
                    }

                    Err(error) => {
                        eprintln!("WAYLAND DISPATCH FAILED {:?}", error);

                        break;
                    }
                }
            }
        });

        receiver
    }
}
