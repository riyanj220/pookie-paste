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
    tracing::info!("starting wlr data control backend");

    let (_, mut event_queue) =
        registry_queue_init::<WaylandState>(&connection).expect("failed creating WLR event queue");

    let qh = event_queue.handle();

    let manager = globals
        .bind::<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .expect("missing zwlr_data_control_manager_v1");

    tracing::debug!("wlr data control manager bound");

    let seat = globals
        .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=10, ())
        .expect("missing wl_seat");

    tracing::debug!("wayland seat bound");

    let device = manager.get_data_device(&seat, &qh, ());

    tracing::debug!("wlr data control device created");

    let mut state = WaylandState {
        _device: device,

        _manager: manager,

        _seat: seat,

        current_offer: None,

        offered_mime_types: Vec::new(),

        sender,

        clipboard_requested: false,

        has_selection: false,
    };

    if let Err(error) = connection.roundtrip() {
        tracing::error!(
            error = ?error,
            "WLR roundtrip failed"
        );

        return;
    }

    tracing::debug!("wlr data control backend initialized");

    loop {
        if let Err(error) = connection.flush() {
            tracing::error!(
                error = ?error,
                "WLR flush failed"
            );
        }

        if let Err(error) = event_queue.blocking_dispatch(&mut state) {
            tracing::error!(
                error = ?error,
                "WLR dispatch failed"
            );

            break;
        }
    }
}
