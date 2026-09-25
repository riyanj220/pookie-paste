use tokio::sync::mpsc;

use tracing::{info, warn};

use crate::platform_shortcut_backend::PlatformShortcutBackend;
use crate::shortcut_backend::{
    Shortcut, ShortcutActivation, ShortcutBackend, ShortcutBackendCapability, ShortcutError,
};
use crate::shortcut_config::ShortcutConfig;

pub struct ShortcutListener {
    receiver: mpsc::UnboundedReceiver<ShortcutActivation>,
}

impl ShortcutListener {
    /// Starts the global shortcut listener using the configured primary shortcut
    /// from `ShortcutConfig` (or default Super+V if unconfigured/invalid).
    pub fn start() -> Self {
        let config = ShortcutConfig::load_or_default();
        let shortcut = config.primary_shortcut().unwrap_or_else(|err| {
            warn!(
                error = %err,
                "failed to parse primary shortcut from config; falling back to default Super+V"
            );
            Shortcut::super_v()
        });

        Self::start_with_shortcut(shortcut)
    }

    /// Starts the global shortcut listener with an explicit `Shortcut` against
    /// the platform's auto-detected shortcut backend.
    pub fn start_with_shortcut(shortcut: Shortcut) -> Self {
        let backend = match PlatformShortcutBackend::new() {
            Ok(backend) => backend,

            Err(error) => {
                warn!(
                    error = ?error,
                    "global shortcut backend unavailable"
                );

                let (_sender, receiver) = mpsc::unbounded_channel();
                return Self { receiver };
            }
        };

        Self::start_with_backend_and_shortcut(backend, shortcut)
    }

    /// Starts the global shortcut listener with an explicit backend and shortcut,
    /// enabling complete deterministic testing and generic platform decoupling.
    pub fn start_with_backend_and_shortcut<B: ShortcutBackend + Send + 'static>(
        mut backend: B,
        shortcut: Shortcut,
    ) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            info!("shortcut backend: {}", backend.name());

            match backend.register(shortcut) {
                Ok(outcome) => {
                    info!("global shortcut registered: {}", outcome.description());
                }
                Err(error) => {
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
            }

            if backend.capability() == ShortcutBackendCapability::CompositorManaged {
                info!(
                    "compositor-managed shortcut backend active; activation is handled via daemon IPC"
                );
                return;
            }

            loop {
                match backend.wait_for_activation() {
                    Ok(activation) => {
                        if sender.send(activation).is_err() {
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

    pub async fn activated(&mut self) -> Option<ShortcutActivation> {
        self.receiver.recv().await
    }
}
