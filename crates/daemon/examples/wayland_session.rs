use daemon::wayland_shortcut_backend::create_global_shortcuts_session;

const PREFERRED_TRIGGER: &str = "<Super>v";

#[tokio::main]
async fn main() {
    println!("Creating GlobalShortcuts portal session...");

    let session = match create_global_shortcuts_session().await {
        Ok(session) => session,

        Err(error) => {
            eprintln!("Unable to create GlobalShortcuts session: {error:?}");

            return;
        }
    };

    println!(
        "GlobalShortcuts session created: {}",
        session.session_handle()
    );

    println!("Binding clipboard history shortcut with preferred trigger: {PREFERRED_TRIGGER}");

    let bound_shortcuts = match session.bind_clipboard_history(PREFERRED_TRIGGER).await {
        Ok(shortcuts) => shortcuts,

        Err(error) => {
            eprintln!("Unable to bind clipboard history shortcut: {error:?}");

            return;
        }
    };

    if bound_shortcuts.is_empty() {
        eprintln!("Portal returned no bound shortcuts");

        return;
    }

    println!("Bound shortcuts:");

    for shortcut in &bound_shortcuts {
        println!(
            "  id: {}, trigger: {}",
            shortcut.id,
            shortcut
                .trigger_description
                .as_deref()
                .unwrap_or("<not provided>")
        );
    }

    println!("Waiting for clipboard history activation...");

    loop {
        let activation = match session.wait_for_clipboard_history_activation().await {
            Ok(activation) => activation,

            Err(error) => {
                eprintln!("Unable to receive shortcut activation: {error:?}");

                return;
            }
        };

        println!(
            "Activated: id={}, timestamp={}, activation_token={:?}",
            activation.shortcut_id, activation.timestamp, activation.activation_token,
        );

        println!("Waiting for matching deactivation...");

        match session.wait_for_clipboard_history_deactivation().await {
            Ok(deactivation) => {
                println!(
                    "Deactivated: id={}, timestamp={}",
                    deactivation.shortcut_id, deactivation.timestamp,
                );
            }

            Err(error) => {
                eprintln!("Unable to receive shortcut deactivation: {error:?}");

                return;
            }
        }

        println!("Waiting for next clipboard history activation...");
    }
}
