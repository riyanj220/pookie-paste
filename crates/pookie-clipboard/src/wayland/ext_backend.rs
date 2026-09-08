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
    tracing::info!("starting ext data control backend");

    let (_, mut event_queue) = registry_queue_init::<ExtDataControlState>(&connection)
        .expect("failed creating EXT event queue");

    let qh = event_queue.handle();

    let manager = globals
        .bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .expect("missing ext_data_control_manager_v1");

    tracing::debug!("ext data control manager bound");

    let seat = globals
        .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())
        .expect("missing wl_seat");

    tracing::debug!("wayland seat bound");

    let device = manager.get_data_device(&seat, &qh, ());

    tracing::debug!("ext data control device created");

    let mut state = ExtDataControlState {
        _manager: manager,

        _device: device,

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
            "wayland EXT roundtrip failed"
        );

        return;
    }

    tracing::debug!("ext data control backend initialized");

    loop {
        if let Err(error) = connection.flush() {
            tracing::error!(
                error = ?error,
                "wayland flush failed"
            );
        }

        if let Err(error) = event_queue.blocking_dispatch(&mut state) {
            tracing::error!(
                error = ?error,
                "EXT wayland dispatch failed"
            );

            break;
        }
    }
}
