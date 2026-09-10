use wayland_client::{
    Connection, Dispatch, QueueHandle, globals::GlobalListContents, protocol::wl_registry,
};

#[derive(Debug, Default)]
pub struct WaylandRegistryState {
    pub has_ext_data_control: bool,

    pub has_wlr_data_control: bool,
}

impl WaylandRegistryState {
    pub fn detect(&mut self, globals: &GlobalListContents) {
        globals.with_list(|list| {
            for global in list {
                tracing::debug!(
                    interface = %global.interface,
                    version = global.version,
                    "wayland global discovered"
                );

                match global.interface.as_str() {
                    "ext_data_control_manager_v1" => {
                        self.has_ext_data_control = true;

                        tracing::debug!("ext_data_control_manager_v1 available");
                    }

                    "zwlr_data_control_manager_v1" => {
                        self.has_wlr_data_control = true;

                        tracing::debug!("zwlr_data_control_manager_v1 available");
                    }

                    _ => {}
                }
            }
        });
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandRegistryState {
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
