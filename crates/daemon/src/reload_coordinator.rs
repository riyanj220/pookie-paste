use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use ipc::ShortcutStatusInfo;

use crate::shortcut_backend::ShortcutError;
use crate::shortcut_config::{ShortcutConfig, ShortcutConfigError};
use crate::shortcut_listener::ShortcutListener;

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
    listener: Arc<ShortcutListener>,
    reload_gate: Mutex<()>,
}

impl ReloadCoordinator {
    pub fn new(listener: Arc<ShortcutListener>) -> Self {
        Self {
            listener,
            reload_gate: Mutex::new(()),
        }
    }

    /// Safely reloads shortcut configuration at runtime.
    ///
    /// Serialized via `reload_gate` so concurrent requests cannot race.
    /// Uses `ShortcutConfig::load_strict()`:
    /// - If config file is missing, resets to default Super+V.
    /// - If config file is malformed, invalid, or unreadable, returns an error and
    ///   preserves the currently active runtime shortcut.
    pub async fn reload(&self) -> Result<ShortcutStatusInfo, ReloadError> {
        let _guard = self.reload_gate.lock().await;

        info!("reloading shortcut configuration");
        let config = ShortcutConfig::load_strict()?;
        let target_shortcut = config.primary_shortcut()?;

        info!(shortcut = %target_shortcut, "applying reloaded shortcut");
        let status = self.listener.reload(target_shortcut).await?;
        Ok(status)
    }
}
