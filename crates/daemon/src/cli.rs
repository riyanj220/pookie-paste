use std::env;
use std::path::Path;
use std::process;

use ipc::{IpcClient, IpcRequest, IpcResponse, socket_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    RunDaemon,
    ToggleUi,
    Help,
    Version,
}

pub fn parse_args() -> CliAction {
    let args: Vec<String> = env::args().skip(1).collect();
    parse_args_from(args)
}

pub fn parse_args_from(args: impl IntoIterator<Item = impl AsRef<str>>) -> CliAction {
    let mut args = args.into_iter();
    let first = match args.next() {
        Some(arg) => arg.as_ref().to_string(),
        None => return CliAction::RunDaemon,
    };

    match first.as_str() {
        "--toggle" | "-t" => CliAction::ToggleUi,
        "--help" | "-h" => CliAction::Help,
        "--version" | "-V" => CliAction::Version,
        unknown => {
            eprintln!("error: unrecognized argument '{unknown}'");
            eprintln!();
            print_help();
            process::exit(2);
        }
    }
}

pub fn print_help() {
    println!("Pookie Paste - Lightweight Linux clipboard history daemon and popup");
    println!();
    println!("USAGE:");
    println!("    pookie-paste [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -t, --toggle     Trigger the clipboard popup (shows UI if not already open)");
    println!("    -h, --help       Print help information");
    println!("    -V, --version    Print version information");
    println!();
    println!("When run without options, pookie-paste starts the background clipboard daemon.");
}

pub fn print_version() {
    println!("pookie-paste {}", env!("CARGO_PKG_VERSION"));
}

pub async fn run_client(action: CliAction) -> anyhow::Result<()> {
    match action {
        CliAction::RunDaemon => Ok(()),
        CliAction::Help => {
            print_help();
            Ok(())
        }
        CliAction::Version => {
            print_version();
            Ok(())
        }
        CliAction::ToggleUi => send_toggle_request().await,
    }
}

pub async fn send_toggle_request() -> anyhow::Result<()> {
    let path = socket_path();
    send_toggle_request_to(&path).await
}

pub async fn send_toggle_request_to(path: &Path) -> anyhow::Result<()> {
    let mut client = match IpcClient::connect(path).await {
        Ok(client) => client,
        Err(err) => {
            eprintln!(
                "Error: Pookie Paste daemon is not running (could not connect to IPC socket at {})",
                path.display()
            );
            eprintln!("Details: {err}");
            process::exit(1);
        }
    };

    match client.send(&IpcRequest::ToggleUi).await {
        Ok(IpcResponse::UiToggled { launched }) => {
            if !launched {
                // UI is already running, which is expected single-instance behavior
            }
            Ok(())
        }
        Ok(IpcResponse::Error { message }) => {
            eprintln!("Error from Pookie Paste daemon: {message}");
            process::exit(1);
        }
        Ok(other) => {
            eprintln!("Unexpected response from daemon: {other:?}");
            process::exit(1);
        }
        Err(err) => {
            eprintln!("Failed to send toggle request to daemon: {err:?}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_args_as_run_daemon() {
        let args: Vec<&str> = vec![];
        assert_eq!(parse_args_from(args), CliAction::RunDaemon);
    }

    #[test]
    fn parses_toggle_flag() {
        assert_eq!(parse_args_from(["--toggle"]), CliAction::ToggleUi);
        assert_eq!(parse_args_from(["-t"]), CliAction::ToggleUi);
    }

    #[test]
    fn parses_help_flag() {
        assert_eq!(parse_args_from(["--help"]), CliAction::Help);
        assert_eq!(parse_args_from(["-h"]), CliAction::Help);
    }

    #[test]
    fn parses_version_flag() {
        assert_eq!(parse_args_from(["--version"]), CliAction::Version);
        assert_eq!(parse_args_from(["-V"]), CliAction::Version);
    }
}
