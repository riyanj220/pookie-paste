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
    pub _device: zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,

    pub _manager: zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,

    pub _seat: wl_seat::WlSeat,

    pub current_offer: Option<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1>,

    pub offered_mime_types: Vec<String>,

    pub sender: Sender<ClipboardEvent>,

    pub clipboard_requested: bool,

    pub has_selection: bool,
}

impl WaylandState {
    fn reset_offer_state(&mut self) {
        /*
         * A new Wayland data offer must start with a fresh
         * MIME/request state.
         *
         * Without this reset, image/png from an earlier
         * image selection can remain in offered_mime_types
         * after the clipboard changes to text. Since Pookie
         * intentionally prefers images over text, that stale
         * MIME could then be requested from the new text
         * owner and produce an empty payload.
         *
         * Clearing current_offer and has_selection also
         * prevents MIME events for the new offer from being
         * accidentally sent to the previous selection.
         */
        self.current_offer = None;

        self.offered_mime_types.clear();

        self.clipboard_requested = false;

        self.has_selection = false;
    }

    fn request_content(&mut self) {
        if self.clipboard_requested {
            return;
        }

        let Some(offer) = self.current_offer.as_ref() else {
            return;
        };

        let Some(preferred) = mime::preferred_content_mime(&self.offered_mime_types) else {
            tracing::debug!(
                offered = ?self.offered_mime_types,
                "no supported WLR clipboard MIME"
            );

            return;
        };

        let requested_mime = preferred.mime_type.to_string();

        let kind = preferred.kind;

        tracing::debug!(
            mime = %requested_mime,
            kind = ?kind,
            "requesting WLR clipboard data"
        );

        let (read_fd, write_fd) = match nix::unistd::pipe() {
            Ok(pipe) => pipe,

            Err(error) => {
                tracing::error!(
                    error = %error,
                    "failed creating WLR clipboard pipe"
                );

                return;
            }
        };

        offer.receive(requested_mime.clone(), write_fd.as_fd());

        drop(write_fd);

        self.clipboard_requested = true;

        let sender = self.sender.clone();

        std::thread::spawn(move || {
            match clipboard_reader::read_clipboard_fd(read_fd, &requested_mime, kind) {
                Ok(content) => {
                    match &content {
                        crate::ClipboardContent::Text(text) => {
                            tracing::debug!(
                                length = text.len(),
                                mime = %requested_mime,
                                "Wayland WLR clipboard text received"
                            );
                        }

                        crate::ClipboardContent::Image(image) => {
                            tracing::debug!(
                                encoded_bytes = image.len(),
                                mime = %requested_mime,
                                "Wayland WLR clipboard image received"
                            );
                        }
                    }

                    clipboard_reader::send_clipboard_event(sender, content);
                }

                Err(error) => {
                    tracing::error!(
                        error = %error,
                        mime = %requested_mime,
                        "failed reading WLR clipboard payload"
                    );
                }
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
                /*
                 * Start this offer with a completely clean
                 * MIME/request state.
                 */
                state.reset_offer_state();

                tracing::debug!(
                    id = ?id.id(),
                    "WLR data offer created; previous offer state reset"
                );
            }

            zwlr_data_control_device_v1::Event::Selection { id } => {
                tracing::debug!(exists = id.is_some(), "WLR selection changed");

                match id {
                    Some(offer) => {
                        state.current_offer = Some(offer);

                        state.clipboard_requested = false;

                        state.has_selection = true;

                        /*
                         * Handles:
                         *
                         * DataOffer
                         * -> Offer MIME(s)
                         * -> Selection
                         */
                        if !state.offered_mime_types.is_empty() {
                            state.request_content();
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
                if mime::is_supported_clipboard_mime(&mime_type) {
                    tracing::debug!(
                        mime = %mime_type,
                        "WLR supported clipboard MIME"
                    );

                    if !state.offered_mime_types.contains(&mime_type) {
                        state.offered_mime_types.push(mime_type);
                    }

                    /*
                     * Handles:
                     *
                     * DataOffer
                     * -> Selection
                     * -> Offer MIME(s)
                     */
                    if state.has_selection {
                        state.request_content();
                    }
                }
            }

            _ => {
                tracing::debug!("ignoring unsupported WLR offer event");
            }
        }
    }
}
