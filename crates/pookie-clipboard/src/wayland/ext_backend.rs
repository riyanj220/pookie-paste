use tokio::sync::mpsc::Sender;

use wayland_client::{
    Connection,
    globals::{GlobalList, registry_queue_init},
    protocol::wl_seat,
};

use crate::ClipboardEvent;

use super::{
    ext_data_control::ExtDataControlState, ext_protocol::client::ext_data_control_manager_v1,
};

pub fn start(connection: Connection, globals: GlobalList, sender: Sender<ClipboardEvent>) {
    println!("STARTING KDE EXT DATA CONTROL BACKEND");

    let (_, mut event_queue) = registry_queue_init::<ExtDataControlState>(&connection)
        .expect("failed creating KDE event queue");

    let qh = event_queue.handle();

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

        offered_mime_types: Vec::new(),

        sender,

        clipboard_requested: false,
    };

    println!("KDE CLIPBOARD STATE CREATED");

    if let Err(error) = connection.roundtrip() {
        eprintln!("WAYLAND ROUNDTRIP FAILED {:?}", error);

        return;
    }

    println!("KDE WAYLAND ROUNDTRIP COMPLETE");

    loop {
        if let Err(error) = connection.flush() {
            eprintln!("WAYLAND FLUSH FAILED {:?}", error);
        }

        match event_queue.blocking_dispatch(&mut state) {
            Ok(_) => {}

            Err(error) => {
                eprintln!("KDE WAYLAND DISPATCH FAILED {:?}", error);

                break;
            }
        }
    }
}
