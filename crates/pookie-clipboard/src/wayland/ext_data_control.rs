use std::{
    fs::File,
    io::Read,
    os::fd::{AsFd, OwnedFd},
};

use tokio::sync::mpsc::Sender;

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::GlobalListContents,
    protocol::{wl_registry, wl_seat},
};

use crate::ClipboardEvent;

use super::ext_protocol::client::{
    ext_data_control_device_v1, ext_data_control_manager_v1, ext_data_control_offer_v1,
};

const SUPPORTED_MIME_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "text/plain",
];

pub struct ExtDataControlState {
    pub manager: ext_data_control_manager_v1::ExtDataControlManagerV1,

    pub device: ext_data_control_device_v1::ExtDataControlDeviceV1,

    pub seat: wl_seat::WlSeat,

    pub current_offer: Option<ext_data_control_offer_v1::ExtDataControlOfferV1>,

    pub offers: Vec<ext_data_control_offer_v1::ExtDataControlOfferV1>,

    pub offered_mime_types: Vec<String>,

    pub sender: Sender<ClipboardEvent>,
}

impl ExtDataControlState {
    fn read_clipboard_fd(fd: OwnedFd) -> Result<String, String> {
        let mut file = File::from(fd);

        let mut contents = String::new();

        file.read_to_string(&mut contents)
            .map_err(|error| format!("failed reading clipboard fd: {error}"))?;

        Ok(contents)
    }

    fn request_text(&mut self) {
        let Some(offer) = self.current_offer.as_ref() else {
            tracing::warn!("cannot request clipboard text: no active offer");

            return;
        };

        let mime = self
            .offered_mime_types
            .iter()
            .find(|mime| SUPPORTED_MIME_TYPES.contains(&mime.as_str()));

        let Some(mime) = mime else {
            tracing::warn!(
                "cannot request clipboard text: unsupported MIME list {:?}",
                self.offered_mime_types
            );

            return;
        };

        tracing::info!("requesting clipboard data mime={}", mime);

        let (read_fd, write_fd) = nix::unistd::pipe().expect("failed creating clipboard pipe");

        offer.receive(mime.clone(), write_fd.as_fd());

        match Self::read_clipboard_fd(read_fd) {
            Ok(value) => {
                tracing::info!("clipboard text received length={}", value.len());

                let event = ClipboardEvent {
                    id: uuid::Uuid::new_v4().to_string(),

                    content: crate::ClipboardContent::Text(value),

                    created_at: chrono::Utc::now(),
                };

                if let Err(error) = self.sender.blocking_send(event) {
                    tracing::error!("failed sending clipboard event {}", error);
                }
            }

            Err(error) => {
                tracing::error!("clipboard read failed {}", error);
            }
        }
    }

    fn dump_state(&self) {
        tracing::info!(
            "clipboard state offers={} mime_types={:?} has_selection={}",
            self.offers.len(),
            self.offered_mime_types,
            self.current_offer.is_some()
        );
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ExtDataControlState {
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

impl Dispatch<wl_seat::WlSeat, ()> for ExtDataControlState {
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

impl Dispatch<ext_data_control_manager_v1::ExtDataControlManagerV1, ()> for ExtDataControlState {
    fn event(
        _state: &mut Self,
        _proxy: &ext_data_control_manager_v1::ExtDataControlManagerV1,
        _event: ext_data_control_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_data_control_device_v1::ExtDataControlDeviceV1, ()> for ExtDataControlState {
    fn event(
        state: &mut Self,
        _proxy: &ext_data_control_device_v1::ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        tracing::info!("KDE device event received {:?}", event);

        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                tracing::info!("new KDE clipboard offer created");

                state.offers.push(id);
            }

            ext_data_control_device_v1::Event::Selection { id } => {
                tracing::info!("KDE clipboard selection changed");

                state.current_offer = id;

                state.offered_mime_types.clear();

                state.dump_state();

                if state.current_offer.is_some() && !state.offered_mime_types.is_empty() {
                    state.request_text();
                }
            }

            _ => {}
        }
    }

    fn event_created_child(
        opcode: u16,
        qhandle: &QueueHandle<Self>,
    ) -> std::sync::Arc<dyn wayland_client::backend::ObjectData> {
        match opcode {
            0 => {
                tracing::info!("creating KDE clipboard offer object");

                qhandle.make_data::<ext_data_control_offer_v1::ExtDataControlOfferV1, ()>(())
            }

            _ => {
                panic!("unknown ext_data_control_device child opcode {}", opcode);
            }
        }
    }
}

impl Dispatch<ext_data_control_offer_v1::ExtDataControlOfferV1, ()> for ExtDataControlState {
    fn event(
        state: &mut Self,
        _proxy: &ext_data_control_offer_v1::ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        tracing::info!("KDE offer event received {:?}", event);

        match event {
            ext_data_control_offer_v1::Event::Offer { mime_type } => {
                tracing::info!("KDE MIME offered {}", mime_type);

                if SUPPORTED_MIME_TYPES.contains(&mime_type.as_str()) {
                    state.offered_mime_types.push(mime_type);

                    state.dump_state();
                }
            }
        }
    }
}
