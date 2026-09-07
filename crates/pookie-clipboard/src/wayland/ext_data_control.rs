use std::{
    fs::File,
    io::Read,
    os::fd::{AsFd, OwnedFd},
};

use tokio::sync::mpsc::Sender;

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
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

    pub offered_mime_types: Vec<String>,

    pub sender: Sender<ClipboardEvent>,

    //
    // KDE transfer state
    //
    pub pending_read_fd: Option<OwnedFd>,

    pub clipboard_requested: bool,
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
        if self.clipboard_requested {
            println!("clipboard request already sent");
            return;
        }

        let Some(offer) = self.current_offer.as_ref() else {
            println!("request_text: no current offer");

            return;
        };

        let Some(mime) = self
            .offered_mime_types
            .iter()
            .find(|mime| SUPPORTED_MIME_TYPES.contains(&mime.as_str()))
        else {
            println!(
                "request_text: unsupported mime {:?}",
                self.offered_mime_types
            );

            return;
        };

        println!("REQUESTING CLIPBOARD mime={}", mime);

        let (read_fd, write_fd) = nix::unistd::pipe().expect("failed creating pipe");

        offer.receive(mime.clone(), write_fd.as_fd());

        drop(write_fd);

        self.pending_read_fd = Some(read_fd);

        self.clipboard_requested = true;

        println!("clipboard fd stored waiting for compositor");
    }

    fn try_read_pending_fd(&mut self) {
        let Some(fd) = self.pending_read_fd.take() else {
            return;
        };

        match Self::read_clipboard_fd(fd) {
            Ok(value) => {
                println!("CLIPBOARD TEXT RECEIVED length={}", value.len());

                let event = ClipboardEvent {
                    id: uuid::Uuid::new_v4().to_string(),

                    content: crate::ClipboardContent::Text(value),

                    created_at: chrono::Utc::now(),
                };

                if let Err(error) = self.sender.blocking_send(event) {
                    println!("FAILED SENDING EVENT {}", error);
                }
            }

            Err(error) => {
                println!("FAILED READING FD {}", error);
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
        println!("DEVICE EVENT {:?}", event);

        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                println!("DATA OFFER CREATED id={:?}", id.id());
            }

            ext_data_control_device_v1::Event::Selection { id } => {
                println!("SELECTION EVENT offer_exists={}", id.is_some());

                state.current_offer = id;

                state.offered_mime_types.clear();

                state.clipboard_requested = false;
            }

            _ => {}
        }
    }

    fn event_created_child(
        opcode: u16,
        qhandle: &QueueHandle<Self>,
    ) -> std::sync::Arc<dyn wayland_client::backend::ObjectData> {
        println!("EVENT CREATED CHILD opcode={}", opcode);

        qhandle.make_data::<ext_data_control_offer_v1::ExtDataControlOfferV1, ()>(())
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
        println!("OFFER EVENT {:?}", event);

        match event {
            ext_data_control_offer_v1::Event::Offer { mime_type } => {
                if SUPPORTED_MIME_TYPES.contains(&mime_type.as_str()) {
                    println!("SUPPORTED MIME RECEIVED {}", mime_type);

                    state.offered_mime_types.push(mime_type);

                    state.request_text();
                }
            }
        }
    }
}
