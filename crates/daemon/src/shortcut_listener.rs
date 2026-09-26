use std::sync::{Arc, RwLock};

use ipc::{
    IpcCompositorBindingStatus, IpcShortcutCapability, IpcShortcutState, ShortcutStatusInfo,
};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::platform_shortcut_backend::PlatformShortcutBackend;
use crate::shortcut_backend::{
    CompositorBindingStatus, Shortcut, ShortcutActivation, ShortcutBackend,
    ShortcutBackendCapability, ShortcutError, ShortcutRegistrationOutcome,
};
use crate::shortcut_config::ShortcutConfig;

pub enum ListenerCommand {
    Rebind {
        shortcut: Shortcut,
        reply_tx: tokio::sync::oneshot::Sender<Result<ShortcutStatusInfo, ShortcutError>>,
    },
    Shutdown,
}

pub type WakeTrigger = Arc<dyn Fn() + Send + Sync>;

pub struct ShortcutListener {
    receiver: mpsc::UnboundedReceiver<ShortcutActivation>,
    status: Arc<RwLock<ShortcutStatusInfo>>,
    command_tx: std::sync::mpsc::Sender<ListenerCommand>,
    wake_trigger: Arc<RwLock<Option<WakeTrigger>>>,
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
                let (command_tx, _command_rx) = std::sync::mpsc::channel();
                let status = Arc::new(RwLock::new(ShortcutStatusInfo {
                    configured_shortcut: shortcut.to_string(),
                    backend_name: None,
                    capability: Some(IpcShortcutCapability::Unsupported),
                    effective_shortcut: None,
                    state: IpcShortcutState::Unavailable {
                        reason: format!("global shortcut backend unavailable: {error}"),
                    },
                }));
                return Self {
                    receiver,
                    status,
                    command_tx,
                    wake_trigger: Arc::new(RwLock::new(None)),
                };
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
        let (command_tx, command_rx) = std::sync::mpsc::channel::<ListenerCommand>();
        let wake_trigger = Arc::new(RwLock::new(backend.wake_trigger()));
        let thread_wake_trigger = Arc::clone(&wake_trigger);

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

            match backend.register(shortcut) {
                Ok(outcome) => {
                    info!("global shortcut registered: {}", outcome.description());
                    apply_outcome_to_status(&backend, &thread_status, shortcut, &outcome);
                    // Refresh wake trigger now that initial registration has succeeded
                    // (essential for backends like Wayland/KDE whose wake channel is initialized during register).
                    if let Ok(mut lock) = thread_wake_trigger.write() {
                        *lock = backend.wake_trigger();
                    }
                }
                Err(error) => {
                    apply_error_to_status(&thread_status, shortcut, &error);
                }
            }

            // Option A: CompositorManaged (Sway, Hyprland).
            // Activation is handled externally/via IPC --toggle.
            // Worker stays alive waiting on command_rx for rebind/shutdown.
            if backend.capability() == ShortcutBackendCapability::CompositorManaged {
                info!(
                    "compositor-managed shortcut backend active; activation is handled via daemon IPC"
                );
                while let Ok(cmd) = command_rx.recv() {
                    match cmd {
                        ListenerCommand::Rebind {
                            shortcut: target,
                            reply_tx,
                        } => {
                            let res = handle_rebind(
                                &mut backend,
                                &thread_status,
                                &thread_wake_trigger,
                                target,
                            );
                            let _ = reply_tx.send(res);
                        }
                        ListenerCommand::Shutdown => break,
                    }
                }
                return;
            }

            loop {
                // 1. Drain pending control commands before blocking
                while let Ok(cmd) = command_rx.try_recv() {
                    match cmd {
                        ListenerCommand::Rebind {
                            shortcut: target,
                            reply_tx,
                        } => {
                            let res = handle_rebind(
                                &mut backend,
                                &thread_status,
                                &thread_wake_trigger,
                                target,
                            );
                            let _ = reply_tx.send(res);
                        }
                        ListenerCommand::Shutdown => return,
                    }
                }

                // 2. Wait for activation or wake
                match backend.wait_for_activation() {
                    Ok(activation) => {
                        if sender.send(activation).is_err() {
                            break;
                        }
                    }

                    Err(ShortcutError::Interrupted) => {
                        // Interrupted by wake_trigger to handle pending control command
                        continue;
                    }

                    Err(ShortcutError::Unavailable) => {
                        warn!("global shortcuts became unavailable");
                        if let Ok(mut lock) = thread_status.write() {
                            lock.state = IpcShortcutState::Unavailable {
                                reason: "global shortcuts became unavailable".to_string(),
                            };
                        }
                        // Stay alive to accept rebind or shutdown
                        while let Ok(cmd) = command_rx.recv() {
                            match cmd {
                                ListenerCommand::Rebind {
                                    shortcut: target,
                                    reply_tx,
                                } => {
                                    let res = handle_rebind(
                                        &mut backend,
                                        &thread_status,
                                        &thread_wake_trigger,
                                        target,
                                    );
                                    let is_ok = res.is_ok();
                                    let _ = reply_tx.send(res);
                                    if is_ok {
                                        break;
                                    }
                                }
                                ListenerCommand::Shutdown => return,
                            }
                        }
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
                        // Stay alive to accept rebind or shutdown
                        while let Ok(cmd) = command_rx.recv() {
                            match cmd {
                                ListenerCommand::Rebind {
                                    shortcut: target,
                                    reply_tx,
                                } => {
                                    let res = handle_rebind(
                                        &mut backend,
                                        &thread_status,
                                        &thread_wake_trigger,
                                        target,
                                    );
                                    let is_ok = res.is_ok();
                                    let _ = reply_tx.send(res);
                                    if is_ok {
                                        break;
                                    }
                                }
                                ListenerCommand::Shutdown => return,
                            }
                        }
                    }
                }
            }
        });

        Self {
            receiver,
            status,
            command_tx,
            wake_trigger,
        }
    }

    /// Rebinds the global shortcut on the background worker thread.
    /// Preserves current runtime state if rebinding fails.
    pub async fn reload(
        &self,
        new_shortcut: Shortcut,
    ) -> Result<ShortcutStatusInfo, ShortcutError> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(ListenerCommand::Rebind {
                shortcut: new_shortcut,
                reply_tx,
            })
            .map_err(|_| {
                ShortcutError::Failed("shortcut listener worker is not running".to_string())
            })?;

        self.wake();

        reply_rx
            .await
            .map_err(|_| ShortcutError::Failed("reload reply channel closed".to_string()))?
    }

    fn wake(&self) {
        if let Ok(guard) = self.wake_trigger.read()
            && let Some(ref wake) = *guard
        {
            wake();
        }
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

impl Drop for ShortcutListener {
    fn drop(&mut self) {
        let _ = self.command_tx.send(ListenerCommand::Shutdown);
        self.wake();
    }
}

fn apply_outcome_to_status<B: ShortcutBackend>(
    backend: &B,
    status: &Arc<RwLock<ShortcutStatusInfo>>,
    shortcut: Shortcut,
    outcome: &ShortcutRegistrationOutcome,
) {
    let (effective_shortcut, state) = match outcome {
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
            (
                effective,
                IpcShortcutState::Active {
                    description: description.clone(),
                },
            )
        }
        ShortcutRegistrationOutcome::CompositorManaged {
            binding_snippet,
            status,
            conflict,
            diagnostic,
        } => {
            let binding_status = match status {
                CompositorBindingStatus::Verified => IpcCompositorBindingStatus::Verified,
                CompositorBindingStatus::BoundUnverified => {
                    IpcCompositorBindingStatus::BoundUnverified
                }
                CompositorBindingStatus::Unconfigured => IpcCompositorBindingStatus::Unconfigured,
                CompositorBindingStatus::Conflict => IpcCompositorBindingStatus::Conflict,
            };
            (
                None,
                IpcShortcutState::CompositorManaged {
                    binding_status,
                    snippet: binding_snippet.clone(),
                    conflict: conflict.clone(),
                    diagnostic: diagnostic.clone(),
                },
            )
        }
        ShortcutRegistrationOutcome::Conflict { details } => (
            None,
            IpcShortcutState::Conflict {
                details: details.clone(),
            },
        ),
    };

    if let Ok(mut lock) = status.write() {
        lock.configured_shortcut = shortcut.to_string();
        lock.effective_shortcut = effective_shortcut;
        lock.state = state;
    }
}

fn apply_error_to_status(
    status: &Arc<RwLock<ShortcutStatusInfo>>,
    shortcut: Shortcut,
    error: &ShortcutError,
) {
    let state = match error {
        ShortcutError::Conflict(message) => {
            warn!(%message, "global shortcut is already in use");
            IpcShortcutState::Conflict {
                details: message.clone(),
            }
        }
        ShortcutError::Unavailable => {
            warn!("global shortcuts are unavailable on this session");
            IpcShortcutState::Unavailable {
                reason: "global shortcuts are unavailable on this session".to_string(),
            }
        }
        ShortcutError::Cancelled => {
            warn!("global shortcut setup was cancelled");
            IpcShortcutState::Failed {
                error: "shortcut registration was cancelled by user".to_string(),
            }
        }
        ShortcutError::Interrupted => {
            return;
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

    if let Ok(mut lock) = status.write() {
        lock.configured_shortcut = shortcut.to_string();
        lock.effective_shortcut = None;
        lock.state = state;
    }
}

fn handle_rebind<B: ShortcutBackend>(
    backend: &mut B,
    status: &Arc<RwLock<ShortcutStatusInfo>>,
    wake_trigger: &Arc<RwLock<Option<WakeTrigger>>>,
    shortcut: Shortcut,
) -> Result<ShortcutStatusInfo, ShortcutError> {
    match backend.rebind(shortcut) {
        Ok(outcome) => {
            info!("global shortcut reloaded: {}", outcome.description());
            apply_outcome_to_status(backend, status, shortcut, &outcome);
            if let Ok(mut lock) = wake_trigger.write() {
                *lock = backend.wake_trigger();
            }
            status
                .read()
                .map(|s| s.clone())
                .map_err(|_| ShortcutError::Failed("status lock poisoned".to_string()))
        }
        Err(err) => {
            warn!(error = ?err, "failed to rebind global shortcut; preserving active shortcut");
            // Important: we do NOT mutate status.configured_shortcut or active state on failure.
            // The running shortcut and its status are preserved!
            Err(err)
        }
    }
}
