use wayland_client::{Connection, Dispatch, QueueHandle};

use wayland_client::protocol::wl_registry;

use wayland_client::globals::GlobalListContents;

use super::ext_protocol::client::{
    ext_data_control_device_v1, ext_data_control_manager_v1, ext_data_control_offer_v1,
};

pub struct ExtDataControlState {
    pub manager: ext_data_control_manager_v1::ExtDataControlManagerV1,

    pub device: ext_data_control_device_v1::ExtDataControlDeviceV1,

    pub current_offer: Option<ext_data_control_offer_v1::ExtDataControlOfferV1>,
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
        _state: &mut Self,

        _proxy: &ext_data_control_device_v1::ExtDataControlDeviceV1,

        event: ext_data_control_device_v1::Event,

        _data: &(),

        _conn: &Connection,

        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::Selection { id: _ } => {
                tracing::info!("KDE Wayland clipboard selection changed");
            }

            _ => {}
        }
    }
}

impl Dispatch<ext_data_control_offer_v1::ExtDataControlOfferV1, ()> for ExtDataControlState {
    fn event(
        _state: &mut Self,

        _proxy: &ext_data_control_offer_v1::ExtDataControlOfferV1,

        event: ext_data_control_offer_v1::Event,

        _data: &(),

        _conn: &Connection,

        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_offer_v1::Event::Offer { mime_type } => {
                tracing::info!("KDE MIME offered: {}", mime_type);
            }

            _ => {}
        }
    }
}
