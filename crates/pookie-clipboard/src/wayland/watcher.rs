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

            tracing::info!("KDE Wayland watcher thread started");

            let connection = match Connection::connect_to_env() {
                Ok(connection) => {
                    println!("CONNECTED TO WAYLAND COMPOSITOR");
                    tracing::info!("Connected to Wayland compositor");

                    connection
                }

                Err(error) => {
                    println!("FAILED CONNECTING WAYLAND: {:?}", error);

                    tracing::error!("failed connecting to Wayland compositor: {:?}", error);

                    return;
                }
            };

            let (globals, mut event_queue) =
                match registry_queue_init::<ExtDataControlState>(&connection) {
                    Ok(value) => {
                        println!("WAYLAND GLOBALS DISCOVERED");

                        tracing::info!("Wayland globals discovered");

                        value
                    }

                    Err(error) => {
                        println!("FAILED INITIALIZING REGISTRY: {:?}", error);

                        tracing::error!("failed initializing registry: {:?}", error);

                        return;
                    }
                };

            let qh = event_queue.handle();

            println!("WAYLAND QUEUE HANDLE CREATED");

            tracing::info!("Wayland queue handle created");

            let manager = match globals
                .bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
            {
                Ok(manager) => {
                    println!("EXT DATA CONTROL MANAGER BOUND");

                    tracing::info!("ext_data_control_manager_v1 bound");

                    manager
                }

                Err(error) => {
                    println!("FAILED BINDING EXT DATA CONTROL MANAGER: {:?}", error);

                    tracing::error!("failed binding ext_data_control_manager_v1: {:?}", error);

                    return;
                }
            };

            let seat = match globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ()) {
                Ok(seat) => {
                    println!("WL SEAT BOUND");

                    tracing::info!("wl_seat bound");

                    seat
                }

                Err(error) => {
                    println!("FAILED BINDING WL SEAT: {:?}", error);

                    tracing::error!("failed binding wl_seat: {:?}", error);

                    return;
                }
            };

            let device = manager.get_data_device(&seat, &qh, ());

            println!("EXT DATA CONTROL DEVICE CREATED");

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

            println!("ENTERING WAYLAND DISPATCH LOOP");

            tracing::info!("entering KDE Wayland dispatch loop");

            loop {
                match event_queue.blocking_dispatch(&mut state) {
                    Ok(_) => {
                        println!("WAYLAND EVENT DISPATCHED");
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
