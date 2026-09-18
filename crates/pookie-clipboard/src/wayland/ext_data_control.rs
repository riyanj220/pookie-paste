use std::os::fd::AsFd;

use tokio::sync::mpsc::Sender;

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::GlobalListContents,
    protocol::{wl_registry, wl_seat},
};

use crate::ClipboardEvent;

use super::{
    clipboard_reader,
    ext_protocol::client::{
        ext_data_control_device_v1, ext_data_control_manager_v1, ext_data_control_offer_v1,
    },
    mime,
};

pub struct ExtDataControlState {
    pub _manager: ext_data_control_manager_v1::ExtDataControlManagerV1,

    pub _device: ext_data_control_device_v1::ExtDataControlDeviceV1,

    pub _seat: wl_seat::WlSeat,

    pub current_offer: Option<ext_data_control_offer_v1::ExtDataControlOfferV1>,

    pub offered_mime_types: Vec<String>,

    pub sender: Sender<ClipboardEvent>,

    pub clipboard_requested: bool,

    pub has_selection: bool,
}

impl ExtDataControlState {
    fn reset_offer_state(&mut self) {
        /*
         * A DataOffer starts a completely new clipboard
         * offer lifecycle.
         *
         * MIME types collected for the previous selection
         * must never be reused with the new offer. Doing so
         * can make an image MIME such as image/png survive
         * an image -> text clipboard transition.
         *
         * Reset current_offer/has_selection as well so MIME
         * events belonging to the new offer cannot
         * accidentally request data from the previous
         * selection while we wait for Selection { ... } to
         * bind the new offer.
         */
        self.current_offer = None;

        self.offered_mime_types.clear();

        self.clipboard_requested = false;

        self.has_selection = false;
    }

    fn request_content(&mut self) {
        if self.clipboard_requested {
            tracing::debug!("clipboard request already sent");

            return;
        }

        let Some(offer) = self.current_offer.as_ref() else {
            tracing::debug!("request_content: no current offer");

            return;
        };

        let Some(preferred) = mime::preferred_content_mime(&self.offered_mime_types) else {
            tracing::debug!(
                offered = ?self.offered_mime_types,
                "no supported clipboard MIME found"
            );

            return;
        };

        let requested_mime = preferred.mime_type.to_string();

        let kind = preferred.kind;

        tracing::debug!(
            mime = %requested_mime,
            kind = ?kind,
            "requesting clipboard data"
        );

        let (read_fd, write_fd) = match nix::unistd::pipe() {
            Ok(pipe) => pipe,

            Err(error) => {
                tracing::error!(
                    error = %error,
                    "failed creating EXT clipboard pipe"
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
                                "Wayland EXT clipboard text received"
                            );
                        }

                        crate::ClipboardContent::Image(image) => {
                            tracing::debug!(
                                encoded_bytes = image.len(),
                                mime = %requested_mime,
                                "Wayland EXT clipboard image received"
                            );
                        }
                    }

                    clipboard_reader::send_clipboard_event(sender, content);
                }

                Err(error) => {
                    tracing::error!(
                        error = %error,
                        mime = %requested_mime,
                        "failed reading EXT clipboard payload"
                    );
                }
            }
        });
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
        tracing::debug!(
            event = ?event,
            "clipboard device event"
        );

        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                /*
                 * Every new data offer owns its own MIME
                 * advertisement set.
                 *
                 * Never carry MIME state from the previous
                 * clipboard selection into this offer.
                 */
                state.reset_offer_state();

                tracing::debug!(
                    id = ?id.id(),
                    "clipboard data offer created; previous offer state reset"
                );
            }

            ext_data_control_device_v1::Event::Selection { id } => {
                tracing::debug!(exists = id.is_some(), "clipboard selection changed");

                match id {
                    Some(offer) => {
                        state.current_offer = Some(offer);

                        state.has_selection = true;

                        state.clipboard_requested = false;

                        /*
                         * Handles:
                         *
                         * DataOffer
                         * -> Offer MIME(s)
                         * -> Selection
                         *
                         * If the MIME events arrived first,
                         * request immediately now that the
                         * selection has been bound.
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
        match event {
            ext_data_control_offer_v1::Event::Offer { mime_type } => {
                if mime::is_supported_clipboard_mime(&mime_type) {
                    tracing::debug!(
                        mime = %mime_type,
                        "supported EXT clipboard MIME received"
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
                     *
                     * If Selection arrived first, request as
                     * soon as a supported MIME is available.
                     */
                    if state.has_selection {
                        state.request_content();
                    }
                }
            }
        }
    }
}
