use std::{
    fs::File,
    io::Read,
    os::fd::{AsFd, OwnedFd},
};

use tokio::sync::mpsc::Sender;

use std::sync::Arc;

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    backend::ObjectData,
    globals::GlobalListContents,
    protocol::{wl_registry, wl_seat},
};

use crate::ClipboardEvent;

use super::ext_protocol::client::{
    ext_data_control_device_v1, ext_data_control_manager_v1, ext_data_control_offer_v1,
};

const SUPPORTED_MIME_TYPES: &[&str] = &["text/plain;charset=utf-8", "text/plain"];

pub struct ExtDataControlState {
    pub manager: ext_data_control_manager_v1::ExtDataControlManagerV1,

    pub device: ext_data_control_device_v1::ExtDataControlDeviceV1,

    pub seat: wl_seat::WlSeat,

    pub current_offer: Option<ext_data_control_offer_v1::ExtDataControlOfferV1>,

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
            return;
        };

        let mime = if self
            .offered_mime_types
            .contains(&"text/plain;charset=utf-8".to_string())
        {
            "text/plain;charset=utf-8"
        } else if self.offered_mime_types.contains(&"text/plain".to_string()) {
            "text/plain"
        } else {
            return;
        };

        let (read_fd, write_fd) = nix::unistd::pipe().expect("failed creating clipboard pipe");

        offer.receive(mime.to_string(), write_fd.as_fd());

        match Self::read_clipboard_fd(read_fd) {
            Ok(value) => {
                let event = ClipboardEvent {
                    id: uuid::Uuid::new_v4().to_string(),

                    content: crate::ClipboardContent::Text(value),

                    created_at: chrono::Utc::now(),
                };

                if let Err(error) = self.sender.blocking_send(event) {
                    tracing::error!("failed sending clipboard event: {}", error);
                }
            }

            Err(error) => {
                tracing::error!("clipboard read failed: {}", error);
            }
        }
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
        match event {
            ext_data_control_device_v1::Event::Selection { id } => {
                state.current_offer = id;

                state.offered_mime_types.clear();

                tracing::info!("KDE Wayland clipboard selection changed");
            }

            _ => {}
        }
    }

    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        match opcode {
            0 => qhandle.make_data::<ext_data_control_offer_v1::ExtDataControlOfferV1, ()>(()),

            _ => {
                tracing::warn!("unknown ext_data_control_device child opcode: {}", opcode);

                qhandle.make_data::<ext_data_control_offer_v1::ExtDataControlOfferV1, ()>(())
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
        match event {
            ext_data_control_offer_v1::Event::Offer { mime_type } => {
                if SUPPORTED_MIME_TYPES.contains(&mime_type.as_str()) {
                    tracing::info!("KDE MIME offered: {}", mime_type);

                    state.offered_mime_types.push(mime_type);

                    state.request_text();
                }
            }
        }
    }
}
