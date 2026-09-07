use wayland_client::{
    Connection, Dispatch, QueueHandle, globals::GlobalListContents, protocol::wl_registry,
};

#[derive(Debug, Default)]
pub struct WaylandRegistryState {
    pub has_ext_data_control: bool,

    pub has_wlr_data_control: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandRegistryState {
    fn event(
        state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            interface,
            version: _,
            name: _,
        } = event
        {
            match interface.as_str() {
                "ext_data_control_manager_v1" => {
                    state.has_ext_data_control = true;

                    tracing::info!("Wayland registry: ext_data_control_manager_v1 available");
                }

                "zwlr_data_control_manager_v1" => {
                    state.has_wlr_data_control = true;

                    tracing::info!("Wayland registry: zwlr_data_control_manager_v1 available");
                }

                _ => {}
            }
        }
    }
}
