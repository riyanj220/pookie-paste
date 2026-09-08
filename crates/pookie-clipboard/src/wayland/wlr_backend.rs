use tokio::sync::mpsc::Sender;

use wayland_client::{
    Connection,
    globals::{GlobalList, registry_queue_init},
    protocol::wl_seat,
};

use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1;

use crate::ClipboardEvent;

use super::wlr_data_control::WaylandState;

pub fn start(connection: Connection, globals: GlobalList, sender: Sender<ClipboardEvent>) {
    println!("STARTING WLR DATA CONTROL BACKEND");

    let (_, mut event_queue) =
        registry_queue_init::<WaylandState>(&connection).expect("failed creating WLR event queue");

    let qh = event_queue.handle();

    let manager = globals
        .bind::<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .expect("missing zwlr_data_control_manager_v1");

    let seat = globals
        .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=10, ())
        .expect("missing wl_seat");

    let device = manager.get_data_device(&seat, &qh, ());

    let mut state = WaylandState {
        device,

        manager,

        seat,

        current_offer: None,

        offered_mime_types: Vec::new(),

        sender,
        clipboard_requested: false,

        has_selection: false,
    };

    if let Err(error) = connection.roundtrip() {
        eprintln!("WLR ROUNDTRIP FAILED {:?}", error);

        return;
    }

    loop {
        if let Err(error) = connection.flush() {
            eprintln!("WLR FLUSH FAILED {:?}", error);
        }

        if let Err(error) = event_queue.blocking_dispatch(&mut state) {
            eprintln!("WLR DISPATCH FAILED {:?}", error);

            break;
        }
    }
}
