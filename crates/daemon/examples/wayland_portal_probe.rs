use daemon::wayland_shortcut_backend::probe_global_shortcuts;

#[tokio::main]
async fn main() {
    match probe_global_shortcuts().await {
        Ok(capability) => {
            println!(
                "GlobalShortcuts portal available, version={}",
                capability.version,
            );
        }

        Err(error) => {
            println!("GlobalShortcuts portal unavailable: {error:?}");
        }
    }
}
