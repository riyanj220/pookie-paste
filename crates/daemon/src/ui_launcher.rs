use std::path::PathBuf;
use std::process::Command;

use tracing::{debug, warn};

#[derive(Debug)]
pub enum UiLaunchError {
    CurrentExecutable(String),
    MissingUiBinary(PathBuf),
    Spawn(String),
}

pub fn launch_ui() -> Result<(), UiLaunchError> {
    let ui_binary = resolve_ui_binary()?;

    if !ui_binary.exists() {
        return Err(UiLaunchError::MissingUiBinary(ui_binary));
    }

    let mut child = Command::new(&ui_binary).spawn().map_err(|error| {
        UiLaunchError::Spawn(format!("failed to launch {}: {error}", ui_binary.display(),))
    })?;

    debug!(
        pid = child.id(),
        path = %ui_binary.display(),
        "Pookie UI launched"
    );

    /*
     * Reap the process after it exits.
     *
     * Dropping Child without waiting can leave a
     * zombie process on Unix while the daemon remains
     * alive.
     */
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            warn!(
                %error,
                "failed waiting for Pookie UI process"
            );
        }
    });

    Ok(())
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
