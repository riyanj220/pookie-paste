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

    pub connection: Connection,

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

        let candidates = mime::candidate_content_mimes(&self.offered_mime_types);
        if candidates.is_empty() {
            tracing::debug!(
                offered = ?self.offered_mime_types,
                "no supported clipboard MIME found"
            );

            return;
        }

        self.clipboard_requested = true;

        let offer = offer.clone();
        let connection = self.connection.clone();
        let sender = self.sender.clone();
        let candidates: Vec<(String, mime::ClipboardMimeKind)> = candidates
            .into_iter()
            .map(|c| (c.mime_type.to_string(), c.kind))
            .collect();

        std::thread::spawn(move || {
            let total = candidates.len();
            for (idx, (requested_mime, kind)) in candidates.iter().enumerate() {
                let is_last = idx + 1 == total;

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

                let _ = connection.flush();

                match clipboard_reader::read_clipboard_fd(read_fd, requested_mime, *kind) {
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
                        return;
                    }

                    Err(error) => {
                        if error == "clipboard payload is empty" {
                            if !is_last {
                                tracing::debug!(
                                    mime = %requested_mime,
                                    "EXT clipboard payload is empty for candidate MIME; trying fallback representation"
                                );
                                continue;
                            } else {
                                tracing::warn!(
                                    mime = %requested_mime,
                                    "all compatible EXT clipboard MIME candidates yielded empty payload"
                                );
                            }
                        } else {
                            if !is_last {
                                tracing::debug!(
                                    error = %error,
                                    mime = %requested_mime,
                                    "candidate EXT clipboard representation failed; trying fallback representation"
                                );
                                continue;
                            } else if *kind == mime::ClipboardMimeKind::FileList {
                                tracing::debug!(
                                    error = %error,
                                    mime = %requested_mime,
                                    "EXT clipboard file-list evaluation yielded no supported image and no fallback was offered"
                                );
                            } else {
                                tracing::error!(
                                    error = %error,
                                    mime = %requested_mime,
                                    "failed reading EXT clipboard payload"
                                );
                            }
                        }
                    }
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
                         * ext-data-control-v1 guarantees that DataOffer and all
                         * associated Offer MIME events are delivered immediately
                         * before this Selection event.
                         *
                         * The Selection event is the authoritative completion
                         * delimiter signalling that the full set of candidate
                         * MIMEs is available.
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
                     * Note: Wayland protocol (ext-data-control-v1) guarantees
                     * that all Offer MIME events are delivered before the
                     * corresponding Selection event. MIME types are accumulated
                     * here, and content is requested once Selection arrives.
                     */
                }
            }
        }
    }
}
