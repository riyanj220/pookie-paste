use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tracing::{debug, warn};

#[derive(Debug)]
pub enum UiLaunchError {
    CurrentExecutable(String),
    MissingUiBinary(PathBuf),
    Spawn(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLaunchOutcome {
    Launched,
    AlreadyRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiStartupMode {
    #[default]
    History,
    ShortcutSetup,
}

impl UiStartupMode {
    pub fn as_arg(&self) -> Option<&'static str> {
        match self {
            Self::History => None,
            Self::ShortcutSetup => Some("--shortcut-setup"),
        }
    }
}

#[derive(Clone)]
pub struct UiLauncher {
    popup_running: Arc<AtomicBool>,
}

impl UiLauncher {
    pub fn new() -> Self {
        Self {
            popup_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.popup_running.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub fn set_running_for_test(&self, running: bool) {
        self.popup_running.store(running, Ordering::Release);
    }

    pub fn launch(&self) -> Result<UiLaunchOutcome, UiLaunchError> {
        self.launch_mode(UiStartupMode::History)
    }

    pub fn launch_mode(&self, mode: UiStartupMode) -> Result<UiLaunchOutcome, UiLaunchError> {
        /*
         * Atomically claim permission to launch the popup.
         *
         * If a popup is already alive, don't launch another
         * process and simply report AlreadyRunning.
         */
        if self
            .popup_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(UiLaunchOutcome::AlreadyRunning);
        }

        let ui_binary = match resolve_ui_binary() {
            Ok(path) => path,

            Err(error) => {
                self.popup_running.store(false, Ordering::Release);

                return Err(error);
            }
        };

        if !ui_binary.exists() {
            self.popup_running.store(false, Ordering::Release);

            return Err(UiLaunchError::MissingUiBinary(ui_binary));
        }

        let mut cmd = Command::new(&ui_binary);
        if let Some(arg) = mode.as_arg() {
            cmd.arg(arg);
        }

        let child = match cmd.spawn() {
            Ok(child) => child,

            Err(error) => {
                self.popup_running.store(false, Ordering::Release);

                return Err(UiLaunchError::Spawn(format!(
                    "failed to launch {}: {error}",
                    ui_binary.display(),
                )));
            }
        };

        debug!(
            pid = child.id(),
            path = %ui_binary.display(),
            "Pookie UI launched"
        );

        let popup_running = Arc::clone(&self.popup_running);

        /*
         * Reap the UI process after it exits.
         *
         * The same lifecycle thread also clears the
         * singleton state, allowing a new popup to be
         * launched after the previous one closes or crashes.
         */
        std::thread::spawn(move || {
            let mut child = child;

            if let Err(error) = child.wait() {
                warn!(
                    %error,
                    "failed waiting for Pookie UI process"
                );
            }

            popup_running.store(false, Ordering::Release);
        });

        Ok(UiLaunchOutcome::Launched)
    }
}

impl Default for UiLauncher {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_ui_binary() -> Result<PathBuf, UiLaunchError> {
    let daemon_binary = std::env::current_exe().map_err(|error| {
        UiLaunchError::CurrentExecutable(format!("failed to resolve daemon executable: {error}"))
    })?;

    let binary_directory = daemon_binary.parent().ok_or_else(|| {
        UiLaunchError::CurrentExecutable("daemon executable has no parent directory".to_string())
    })?;

    Ok(binary_directory.join(ui_binary_name()))
}

fn ui_binary_name() -> &'static str {
    if cfg!(windows) {
        "pookie-paste-ui.exe"
    } else {
        "pookie-paste-ui"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_mode_as_arg_mapping() {
        assert_eq!(UiStartupMode::History.as_arg(), None);
        assert_eq!(
            UiStartupMode::ShortcutSetup.as_arg(),
            Some("--shortcut-setup")
        );
        assert_eq!(UiStartupMode::default(), UiStartupMode::History);
    }
}
