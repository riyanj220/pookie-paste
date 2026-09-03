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

pub struct UiLauncher {
    popup_running: Arc<AtomicBool>,
}

impl UiLauncher {
    pub fn new() -> Self {
        Self {
            popup_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn launch(&self) -> Result<UiLaunchOutcome, UiLaunchError> {
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

        let child = match Command::new(&ui_binary).spawn() {
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
    if cfg!(windows) { "ui.exe" } else { "ui" }
}
