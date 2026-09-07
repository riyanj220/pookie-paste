use wayland_client::{
    Connection, Dispatch, QueueHandle, globals::GlobalListContents, protocol::wl_registry,
};

#[derive(Debug, Default)]
pub struct WaylandRegistryState {
    pub has_ext_data_control: bool,

    pub has_wlr_data_control: bool,
}

impl WaylandRegistryState {
    pub fn log_state(&self) {
        println!(
            "WAYLAND REGISTRY RESULT ext={} wlr={}",
            self.has_ext_data_control, self.has_wlr_data_control
        );
    }
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
        match event {
            wl_registry::Event::Global {
                interface,
                version,
                name,
            } => {
                println!(
                    "WAYLAND GLOBAL FOUND interface={} version={} name={}",
                    interface, version, name
                );

                match interface.as_str() {
                    "ext_data_control_manager_v1" => {
                        state.has_ext_data_control = true;

                        println!("FOUND KDE ext_data_control_manager_v1");
                    }

                    "zwlr_data_control_manager_v1" => {
                        state.has_wlr_data_control = true;

                        println!("FOUND WLR zwlr_data_control_manager_v1");
                    }

                    _ => {}
                }
            }

            _ => {}
        }
    }
}
