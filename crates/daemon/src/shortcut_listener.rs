use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::platform_shortcut_backend::PlatformShortcutBackend;
use crate::shortcut_backend::{Shortcut, ShortcutBackend, ShortcutError};

pub struct ShortcutListener {
    receiver: mpsc::UnboundedReceiver<()>,
}

impl ShortcutListener {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            let mut backend = match PlatformShortcutBackend::new() {
                Ok(backend) => backend,

                Err(error) => {
                    warn!(
                        error = ?error,
                        "global shortcut backend unavailable"
                    );

                    return;
                }
            };

            info!("shortcut backend: {}", backend.name());

            if let Err(error) = backend.register(Shortcut::super_v()) {
                match error {
                    ShortcutError::Conflict(message) => {
                        warn!(
                            %message,
                            "global shortcut is already in use"
                        );
                    }

                    ShortcutError::Unavailable => {
                        warn!("global shortcuts are unavailable on this session");
                    }

                    ShortcutError::Cancelled => {
                        warn!("global shortcut setup was cancelled");
                    }

                    ShortcutError::TimedOut(message) => {
                        warn!(
                            %message,
                            "global shortcut setup timed out"
                        );
                    }

                    ShortcutError::Failed(message) => {
                        warn!(
                            %message,
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

                    Err(ShortcutError::Unavailable) => {
                        warn!("global shortcuts became unavailable");

                        break;
                    }

                    Err(ShortcutError::Cancelled) => {
                        warn!("global shortcut operation was cancelled");

                        break;
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
