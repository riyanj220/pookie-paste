use std::os::fd::AsFd;

use tokio::sync::mpsc::Sender;

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::GlobalListContents,
    protocol::{wl_registry, wl_seat},
};

use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1, zwlr_data_control_manager_v1, zwlr_data_control_offer_v1,
};

use crate::ClipboardEvent;

use super::{clipboard_reader, mime};

pub struct WaylandState {
    pub device: zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,

    pub manager: zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,

    pub seat: wl_seat::WlSeat,

    pub current_offer: Option<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1>,

    pub offered_mime_types: Vec<String>,

    pub sender: Sender<ClipboardEvent>,

    pub clipboard_requested: bool,

    pub has_selection: bool,
}

impl WaylandState {
    fn request_text(&mut self) {
        if self.clipboard_requested {
            return;
        }

        let Some(offer) = self.current_offer.as_ref() else {
            return;
        };

        let Some(mime) = mime::preferred_text_mime(&self.offered_mime_types) else {
            tracing::debug!(
                offered = ?self.offered_mime_types,
                "no supported WLR mime"
            );

            return;
        };

        tracing::debug!(
            mime = %mime,
            "requesting WLR clipboard"
        );

        let (read_fd, write_fd) = nix::unistd::pipe().expect("failed creating clipboard pipe");

        offer.receive(mime.to_string(), write_fd.as_fd());

        drop(write_fd);

        self.clipboard_requested = true;

        let sender = self.sender.clone();

        std::thread::spawn(move || match clipboard_reader::read_clipboard_fd(read_fd) {
            Ok(value) => {
                clipboard_reader::send_clipboard_event(sender, value);
            }

            Err(error) => {
                tracing::error!(
                    error = %error,
                    "failed reading WLR clipboard"
                );
            }
        });
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
        _event: zwlr_data_control_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_data_control_device_v1::ZwlrDataControlDeviceV1, ()> for WaylandState {
    fn event(
        state: &mut Self,

        _proxy: &zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,

        event: zwlr_data_control_device_v1::Event,

        _data: &(),

        _conn: &Connection,

        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                tracing::debug!(
                    id = ?id.id(),
                    "WLR data offer created"
                );
            }

            zwlr_data_control_device_v1::Event::Selection { id } => {
                tracing::debug!(exists = id.is_some(), "WLR selection changed");

                match id {
                    Some(offer) => {
                        state.current_offer = Some(offer);

                        state.clipboard_requested = false;

                        state.has_selection = true;

                        if !state.offered_mime_types.is_empty() {
                            state.request_text();
                        }
                    }

                    None => {
                        state.current_offer = None;

                        state.offered_mime_types.clear();

                        state.clipboard_requested = false;

                        state.has_selection = false;
                    }
                }
            }

            _ => {}
        }
    }

    fn event_created_child(
        _opcode: u16,

        qhandle: &QueueHandle<Self>,
    ) -> std::sync::Arc<dyn wayland_client::backend::ObjectData> {
        qhandle.make_data::<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ()>(())
    }
}

impl Dispatch<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ()> for WaylandState {
    fn event(
        state: &mut Self,

        _proxy: &zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,

        event: zwlr_data_control_offer_v1::Event,

        _data: &(),

        _conn: &Connection,

        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_offer_v1::Event::Offer { mime_type } => {
                if mime::is_supported_text_mime(&mime_type) {
                    tracing::debug!(
                        mime = %mime_type,
                        "WLR supported mime"
                    );

                    if !state.offered_mime_types.contains(&mime_type) {
                        state.offered_mime_types.push(mime_type);
                    }

                    if state.has_selection {
                        state.request_text();
                    }
                }
            }

            _ => {
                tracing::debug!("ignoring unsupported WLR offer event");
            }
        }
    }
}
