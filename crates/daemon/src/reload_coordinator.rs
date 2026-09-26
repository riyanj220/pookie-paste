use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use tracing::info;

use ipc::ShortcutStatusInfo;

use crate::shortcut_backend::ShortcutError;
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

    pub fn status(&self) -> ShortcutStatusInfo {
        self.reload_handle.status()
    }

    pub fn status_handle(&self) -> Arc<RwLock<ShortcutStatusInfo>> {
        self.reload_handle.status_handle()
    }
}
