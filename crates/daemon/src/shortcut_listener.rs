use std::sync::{Arc, RwLock};

use ipc::{IpcShortcutCapability, IpcShortcutState, ShortcutStatusInfo};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::platform_shortcut_backend::PlatformShortcutBackend;
use crate::shortcut_backend::{
    Shortcut, ShortcutActivation, ShortcutBackend, ShortcutBackendCapability, ShortcutError,
    ShortcutRegistrationOutcome,
};
use crate::shortcut_config::ShortcutConfig;

pub struct ShortcutListener {
    receiver: mpsc::UnboundedReceiver<ShortcutActivation>,
    status: Arc<RwLock<ShortcutStatusInfo>>,
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
                let status = Arc::new(RwLock::new(ShortcutStatusInfo {
                    configured_shortcut: shortcut.to_string(),
                    backend_name: None,
                    capability: Some(IpcShortcutCapability::Unsupported),
                    effective_shortcut: None,
                    state: IpcShortcutState::Unavailable {
                        reason: format!("global shortcut backend unavailable: {error}"),
                    },
                }));
                return Self { receiver, status };
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

        let initial_capability = match backend.capability() {
            ShortcutBackendCapability::Native => IpcShortcutCapability::Native,
            ShortcutBackendCapability::Portal => IpcShortcutCapability::Portal,
            ShortcutBackendCapability::CompositorManaged => {
                IpcShortcutCapability::CompositorManaged
            }
            ShortcutBackendCapability::Unsupported => IpcShortcutCapability::Unsupported,
        };

        let status = Arc::new(RwLock::new(ShortcutStatusInfo {
            configured_shortcut: shortcut.to_string(),
            backend_name: Some(backend.name().to_string()),
            capability: Some(initial_capability),
            effective_shortcut: None,
            state: IpcShortcutState::Initializing,
        }));

        let thread_status = Arc::clone(&status);

        std::thread::spawn(move || {
            info!("shortcut backend: {}", backend.name());

            let (effective_shortcut, state) = match backend.register(shortcut) {
                Ok(outcome) => {
                    info!("global shortcut registered: {}", outcome.description());
                    match outcome {
                        ShortcutRegistrationOutcome::Active { description } => {
                            let effective = backend
                                .effective_trigger()
                                .map(ToString::to_string)
                                .or_else(|| {
                                    if backend.capability() == ShortcutBackendCapability::Native {
                                        Some(shortcut.to_string())
                                    } else {
                                        None
                                    }
                                });
                            (effective, IpcShortcutState::Active { description })
                        }
                        ShortcutRegistrationOutcome::CompositorManaged {
                            binding_snippet,
                            verified,
                            conflict,
                            diagnostic,
                        } => (
                            None,
                            IpcShortcutState::CompositorManaged {
                                verified,
                                snippet: binding_snippet,
                                conflict,
                                diagnostic,
                            },
                        ),
                        ShortcutRegistrationOutcome::Conflict { details } => {
                            (None, IpcShortcutState::Conflict { details })
                        }
                    }
                }
                Err(error) => {
                    let state = match &error {
                        ShortcutError::Conflict(message) => {
                            warn!(%message, "global shortcut is already in use");
                            IpcShortcutState::Conflict {
                                details: message.clone(),
                            }
                        }
                        ShortcutError::Unavailable => {
                            warn!("global shortcuts are unavailable on this session");
                            IpcShortcutState::Unavailable {
                                reason: "global shortcuts are unavailable on this session"
                                    .to_string(),
                            }
                        }
                        ShortcutError::Cancelled => {
                            warn!("global shortcut setup was cancelled");
                            IpcShortcutState::Failed {
                                error: "shortcut registration was cancelled by user".to_string(),
                            }
                        }
                        ShortcutError::TimedOut(message) => {
                            warn!(%message, "global shortcut setup timed out");
                            IpcShortcutState::Failed {
                                error: format!("registration timed out: {message}"),
                            }
                        }
                        ShortcutError::Failed(message) => {
                            warn!(%message, "failed to register global shortcut");
                            IpcShortcutState::Failed {
                                error: message.clone(),
                            }
                        }
                    };
                    (None, state)
                }
            };

            let is_failed = matches!(
                state,
                IpcShortcutState::Failed { .. }
                    | IpcShortcutState::Unavailable { .. }
                    | IpcShortcutState::Conflict { .. }
            );

            if let Ok(mut lock) = thread_status.write() {
                lock.effective_shortcut = effective_shortcut;
                lock.state = state;
            }

            if is_failed {
                return;
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
                        if let Ok(mut lock) = thread_status.write() {
                            lock.state = IpcShortcutState::Unavailable {
                                reason: "global shortcuts became unavailable".to_string(),
                            };
                        }
                        break;
                    }

                    Err(ShortcutError::Cancelled) => {
                        warn!("global shortcut operation was cancelled");
                        if let Ok(mut lock) = thread_status.write() {
                            lock.state = IpcShortcutState::Unavailable {
                                reason: "global shortcut operation was cancelled".to_string(),
                            };
                        }
                        break;
                    }

                    Err(error) => {
                        warn!(
                            error = ?error,
                            "global shortcut listener stopped"
                        );
                        if let Ok(mut lock) = thread_status.write() {
                            lock.state = IpcShortcutState::Failed {
                                error: error.to_string(),
                            };
                        }
                        break;
                    }
                }
            }
        });

        Self { receiver, status }
    }

    pub fn status(&self) -> ShortcutStatusInfo {
        self.status
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| ShortcutStatusInfo {
                configured_shortcut: String::new(),
                backend_name: None,
                capability: None,
                effective_shortcut: None,
                state: IpcShortcutState::Unavailable {
                    reason: "status lock poisoned".to_string(),
                },
            })
    }

    pub fn status_handle(&self) -> Arc<RwLock<ShortcutStatusInfo>> {
        Arc::clone(&self.status)
    }

    pub async fn activated(&mut self) -> Option<ShortcutActivation> {
        self.receiver.recv().await
    }
}
