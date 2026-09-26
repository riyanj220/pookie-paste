use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use tracing::{error, info};

use ipc::{IpcShortcutCapability, ShortcutStatusInfo};

use crate::shortcut_backend::{Shortcut, ShortcutError};
use crate::shortcut_config::{ShortcutConfig, ShortcutConfigError};
use crate::shortcut_listener::ShortcutReloadHandle;

#[derive(Debug)]
pub enum ReloadError {
    Config(ShortcutConfigError),
    Shortcut(ShortcutError),
}

impl std::fmt::Display for ReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "configuration error: {e}"),
            Self::Shortcut(e) => write!(f, "shortcut reload failed: {e}"),
        }
    }
}

impl std::error::Error for ReloadError {}

impl From<ShortcutConfigError> for ReloadError {
    fn from(err: ShortcutConfigError) -> Self {
        Self::Config(err)
    }
}

impl From<ShortcutError> for ReloadError {
    fn from(err: ShortcutError) -> Self {
        Self::Shortcut(err)
    }
}

pub struct ReloadCoordinator {
    reload_handle: ShortcutReloadHandle,
    reload_gate: Mutex<()>,
    custom_paths: Option<(PathBuf, PathBuf)>,
}

impl ReloadCoordinator {
    pub fn new(reload_handle: ShortcutReloadHandle) -> Self {
        Self {
            reload_handle,
            reload_gate: Mutex::new(()),
            custom_paths: None,
        }
    }

    /// Creates a reload coordinator with explicit config paths, enabling fully isolated
    /// testing without mutating process-wide environment variables.
    pub fn with_custom_paths(
        reload_handle: ShortcutReloadHandle,
        resolved_path: PathBuf,
        default_path: PathBuf,
    ) -> Self {
        Self {
            reload_handle,
            reload_gate: Mutex::new(()),
            custom_paths: Some((resolved_path, default_path)),
        }
    }

    /// Safely reloads shortcut configuration at runtime.
    ///
    /// Serialized via `reload_gate` so concurrent requests cannot race.
    /// Uses `ShortcutConfig::load_strict()` (or injected test paths):
    /// - If config file is missing, bootstraps canonical file and resets to default Super+V.
    /// - If config file is malformed, invalid, or unreadable, returns an error and
    ///   preserves the currently active runtime shortcut.
    pub async fn reload(&self) -> Result<ShortcutStatusInfo, ReloadError> {
        let _guard = self.reload_gate.lock().await;

        info!("reloading shortcut configuration");
        let config = match &self.custom_paths {
            Some((resolved, default)) => ShortcutConfig::load_strict_with_paths(resolved, default)?,
            None => ShortcutConfig::load_strict()?,
        };
        let target_shortcut = config.primary_shortcut()?;

        info!(shortcut = %target_shortcut, "applying reloaded shortcut");
        let status = self.reload_handle.reload(target_shortcut).await?;
        Ok(status)
    }

    /// Atomically sets and persists a new shortcut with capability-specific transaction semantics.
    pub async fn set_shortcut(
        &self,
        new_shortcut: Shortcut,
    ) -> Result<ShortcutStatusInfo, ReloadError> {
        let _guard = self.reload_gate.lock().await;

        let current_status = self.reload_handle.status();
        let capability = current_status.capability.ok_or_else(|| {
            ShortcutConfigError::Parse("shortcut capability is unknown".to_string())
        })?;

        match capability {
            IpcShortcutCapability::Portal => Err(ReloadError::Shortcut(ShortcutError::Failed(
                "Shortcut is managed by the desktop portal; use ConfigurePortalShortcut"
                    .to_string(),
            ))),
            IpcShortcutCapability::Unsupported => {
                Err(ReloadError::Shortcut(ShortcutError::Unavailable))
            }
            IpcShortcutCapability::Native => {
                let target_path = match &self.custom_paths {
                    Some((_resolved, default)) => default.clone(),
                    None => ShortcutConfig::config_path()
                        .map_err(|e| ReloadError::Config(ShortcutConfigError::Io(e.to_string())))?,
                };

                // Phase 1: Prepare updated config in a sibling temp file
                let temp_path =
                    ShortcutConfig::prepare_new_config_file(&target_path, new_shortcut)?;

                // Phase 2: Attempt runtime rebind
                let rebind_result = self.reload_handle.reload(new_shortcut).await;
                if let Err(rebind_err) = rebind_result {
                    // Rebind failed; discard temp file.
                    // Previous runtime is preserved by worker rollback, config is untouched.
                    ShortcutConfig::clean_prepared_file(&temp_path);
                    return Err(ReloadError::Shortcut(rebind_err));
                }

                // Phase 3: Atomically commit prepared file
                if let Err(commit_err) =
                    ShortcutConfig::commit_prepared_file(&temp_path, &target_path)
                {
                    // Config commit failed after successful rebind! Attempt rollback to previous shortcut.
                    ShortcutConfig::clean_prepared_file(&temp_path);

                    let previous_shortcut = match ShortcutConfig::load_from_path(&target_path) {
                        Ok(cfg) => cfg
                            .primary_shortcut()
                            .unwrap_or_else(|_| Shortcut::super_v()),
                        Err(_) => Shortcut::super_v(),
                    };

                    match self.reload_handle.reload(previous_shortcut).await {
                        Ok(_) => {
                            // Rollback succeeded: runtime = old, config = old
                            return Err(ReloadError::Config(commit_err));
                        }
                        Err(rollback_err) => {
                            // Double failure case: rollback ALSO failed!
                            error!(
                                %commit_err,
                                %rollback_err,
                                "failed to commit configuration and rollback to previous runtime shortcut also failed"
                            );
                            return Err(ReloadError::Shortcut(ShortcutError::Failed(format!(
                                "failed to commit configuration: {commit_err}; rollback to previous runtime shortcut also failed: {rollback_err}; current runtime shortcut may differ from persisted configuration"
                            ))));
                        }
                    }
                }

                // Commit succeeded: runtime = new, config = new
                Ok(self.reload_handle.status())
            }
            IpcShortcutCapability::CompositorManaged => {
                let target_path = match &self.custom_paths {
                    Some((_resolved, default)) => default.clone(),
                    None => ShortcutConfig::config_path()
                        .map_err(|e| ReloadError::Config(ShortcutConfigError::Io(e.to_string())))?,
                };

                // For compositor-managed (Sway, Hyprland):
                // User is persisting their desired shortcut. Prepare and commit to config.toml,
                // then re-probe compositor to update the status snippet.
                let temp_path =
                    ShortcutConfig::prepare_new_config_file(&target_path, new_shortcut)?;
                ShortcutConfig::commit_prepared_file(&temp_path, &target_path)?;

                // Re-probe compositor via worker
                let status = self.reload_handle.reload(new_shortcut).await?;
                Ok(status)
            }
        }
    }

    /// Triggers portal shortcut configuration on the background worker (KDE Plasma).
    pub async fn configure_portal(&self) -> Result<ShortcutStatusInfo, ReloadError> {
        let _guard = self.reload_gate.lock().await;

        let status = self.reload_handle.configure_portal(None).await?;
        Ok(status)
    }

    pub fn status(&self) -> ShortcutStatusInfo {
        self.reload_handle.status()
    }

    pub fn status_handle(&self) -> Arc<RwLock<ShortcutStatusInfo>> {
        self.reload_handle.status_handle()
    }
}
