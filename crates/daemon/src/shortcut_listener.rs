use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::shortcut_backend::{Shortcut, ShortcutBackend, ShortcutError};

use crate::x11_shortcut_backend::X11ShortcutBackend;

pub struct ShortcutListener {
    receiver: mpsc::UnboundedReceiver<()>,
}

impl ShortcutListener {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            let mut backend = match X11ShortcutBackend::new() {
                Ok(backend) => backend,

                Err(error) => {
                    warn!(
                        error = ?error,
                        "global shortcut backend unavailable"
                    );

                    return;
                }
            };

            if let Err(error) = backend.register(Shortcut::super_v()) {
                match error {
                    ShortcutError::Conflict(message) => {
                        warn!(
                            %message,
                            "global shortcut is already in use"
                        );
                    }

                    other => {
                        warn!(
                            error = ?other,
                            "failed to register global shortcut"
                        );
                    }
                }

                return;
            }

            info!("global shortcut registered: Super+V");

            loop {
                match backend.wait_for_activation() {
                    Ok(()) => {
                        if sender.send(()).is_err() {
                            break;
                        }
                    }

                    Err(error) => {
                        warn!(
                            error = ?error,
                            "global shortcut listener stopped"
                        );

                        break;
                    }
                }
            }
        });

        Self { receiver }
    }

    pub async fn activated(&mut self) -> Option<()> {
        self.receiver.recv().await
    }
}
