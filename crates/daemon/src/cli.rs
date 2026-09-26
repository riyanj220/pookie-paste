use std::env;
use std::path::Path;
use std::process;

use ipc::{
    IpcClient, IpcCompositorBindingStatus, IpcRequest, IpcResponse, IpcShortcutCapability,
    IpcShortcutState, ShortcutStatusInfo, socket_path,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    RunDaemon,
    ToggleUi,
    ShortcutStatus,
    ReloadConfig,
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
        "--shortcut-status" => CliAction::ShortcutStatus,
        "--reload" | "-r" => CliAction::ReloadConfig,
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
    println!(
        "    -t, --toggle             Trigger the clipboard popup (shows UI if not already open)"
    );
    println!(
        "    -r, --reload             Reload configuration and shortcut in the running daemon"
    );
    println!(
        "        --shortcut-status    Print global shortcut configuration and active runtime status"
    );
    println!("    -h, --help               Print help information");
    println!("    -V, --version            Print version information");
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
        CliAction::ShortcutStatus => send_shortcut_status_request().await,
        CliAction::ReloadConfig => send_reload_request().await,
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

pub async fn send_shortcut_status_request() -> anyhow::Result<()> {
    let path = socket_path();
    send_shortcut_status_request_to(&path).await
}

pub async fn send_shortcut_status_request_to(path: &Path) -> anyhow::Result<()> {
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

    match client.send(&IpcRequest::GetShortcutStatus).await {
        Ok(IpcResponse::ShortcutStatus { status }) => {
            print!("{}", format_shortcut_status(&status));
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
            eprintln!("Failed to send shortcut status request to daemon: {err:?}");
            process::exit(1);
        }
    }
}

pub async fn send_reload_request() -> anyhow::Result<()> {
    let path = socket_path();
    send_reload_request_to(&path).await
}

pub async fn send_reload_request_to(path: &Path) -> anyhow::Result<()> {
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

    match client.send(&IpcRequest::ReloadConfig).await {
        Ok(IpcResponse::ConfigReloaded { status }) => {
            print!("{}", format_reload_status(&status));
            Ok(())
        }
        Ok(IpcResponse::Error { message }) => {
            eprintln!("Error: Configuration reload failed: {message}");
            eprintln!("Current runtime shortcut and active bindings have been preserved.");
            process::exit(1);
        }
        Ok(other) => {
            eprintln!("Unexpected response from daemon: {other:?}");
            process::exit(1);
        }
        Err(err) => {
            eprintln!("Failed to send reload request to daemon: {err:?}");
            process::exit(1);
        }
    }
}

pub fn format_shortcut_status(status: &ShortcutStatusInfo) -> String {
    format_shortcut_status_with_header("Pookie Paste Global Shortcut Status", status)
}

pub fn format_reload_status(status: &ShortcutStatusInfo) -> String {
    format_shortcut_status_with_header("Pookie Paste Configuration Reloaded", status)
}

pub fn format_shortcut_status_with_header(header: &str, status: &ShortcutStatusInfo) -> String {
    let mut out = String::new();
    out.push_str(header);
    out.push('\n');
    out.push_str(&"-".repeat(header.len()));
    out.push('\n');
    out.push_str(&format!(
        "Configured Shortcut : {}\n",
        status.configured_shortcut
    ));

    if let Some(backend) = &status.backend_name {
        out.push_str(&format!("Backend             : {}\n", backend));
    }

    if let Some(capability) = status.capability {
        let cap_str = match capability {
            IpcShortcutCapability::Native => "Native window system key grab (e.g. X11)",
            IpcShortcutCapability::Portal => "Desktop portal global shortcuts (e.g. KDE Plasma)",
            IpcShortcutCapability::CompositorManaged => {
                "Compositor-managed keybinding (e.g. Sway, Hyprland)"
            }
            IpcShortcutCapability::Unsupported => "Unsupported on active session",
        };
        out.push_str(&format!("Capability          : {}\n", cap_str));
    }

    match &status.state {
        IpcShortcutState::Initializing => {
            out.push_str("Status              : Initializing (registration in progress)\n");
        }
        IpcShortcutState::Active { description } => {
            out.push_str("Status              : Active\n");
            if let Some(effective) = &status.effective_shortcut {
                out.push_str(&format!("Effective Shortcut  : {}\n", effective));
            }
            out.push_str(&format!("Details             : {}\n", description));
        }
        IpcShortcutState::CompositorManaged {
            binding_status,
            snippet,
            conflict,
            diagnostic,
        } => {
            let status_str = match binding_status {
                IpcCompositorBindingStatus::Verified => "Verified",
                IpcCompositorBindingStatus::BoundUnverified => "Bound / Unverified",
                IpcCompositorBindingStatus::Unconfigured => "Unconfigured / Missing",
                IpcCompositorBindingStatus::Conflict => "Conflict",
            };
            out.push_str(&format!("Status              : {}\n", status_str));
            out.push_str(&format!("Binding Directive   : {}\n", snippet));
            if let Some(c) = conflict {
                out.push_str(&format!("Conflict Detected   : {}\n", c));
            }
            if let Some(d) = diagnostic {
                out.push_str(&format!("Diagnostic          : {}\n", d));
            }
            match binding_status {
                IpcCompositorBindingStatus::Unconfigured => {
                    out.push_str(
                        "Action Required     : Add the binding directive above to your compositor configuration.\n",
                    );
                }
                IpcCompositorBindingStatus::Conflict => {
                    out.push_str(
                        "Action Required     : Resolve the conflicting shortcut in your compositor configuration.\n",
                    );
                }
                IpcCompositorBindingStatus::BoundUnverified => {
                    out.push_str(
                        "Guidance            : Ensure the active binding executes 'pookie-paste --toggle'.\n",
                    );
                }
                IpcCompositorBindingStatus::Verified => {}
            }
        }
        IpcShortcutState::Conflict { details } => {
            out.push_str("Status              : Conflict\n");
            out.push_str(&format!("Details             : {}\n", details));
        }
        IpcShortcutState::Unavailable { reason } => {
            out.push_str("Status              : Unavailable\n");
            out.push_str(&format!("Reason              : {}\n", reason));
        }
        IpcShortcutState::Failed { error } => {
            out.push_str("Status              : Failed\n");
            out.push_str(&format!("Error               : {}\n", error));
        }
    }

    out
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

    #[test]
    fn parses_shortcut_status_flag() {
        assert_eq!(
            parse_args_from(["--shortcut-status"]),
            CliAction::ShortcutStatus
        );
    }

    #[test]
    fn parses_reload_flag() {
        assert_eq!(parse_args_from(["--reload"]), CliAction::ReloadConfig);
        assert_eq!(parse_args_from(["-r"]), CliAction::ReloadConfig);
    }

    #[test]
    fn formats_reload_status_header() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("X11 global shortcut".to_string()),
            capability: Some(IpcShortcutCapability::Native),
            effective_shortcut: Some("Super+V".to_string()),
            state: IpcShortcutState::Active {
                description: "X11 root window grab for Super+V".to_string(),
            },
        };
        let formatted = format_reload_status(&status);
        assert!(formatted.contains("Pookie Paste Configuration Reloaded"));
        assert!(formatted.contains("-----------------------------------"));
        assert!(formatted.contains("Configured Shortcut : Super+V"));
    }

    #[test]
    fn formats_initializing_shortcut_status() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: None,
            capability: None,
            effective_shortcut: None,
            state: IpcShortcutState::Initializing,
        };
        let formatted = format_shortcut_status(&status);
        assert!(formatted.contains("Configured Shortcut : Super+V"));
        assert!(
            formatted.contains("Status              : Initializing (registration in progress)")
        );
    }

    #[test]
    fn formats_kde_portal_with_differing_effective_trigger() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Ctrl+Shift+P".to_string(),
            backend_name: Some(
                "KDE Plasma GlobalShortcuts Portal (XDG Desktop Portal v2)".to_string(),
            ),
            capability: Some(IpcShortcutCapability::Portal),
            effective_shortcut: Some("Meta+V".to_string()),
            state: IpcShortcutState::Active {
                description:
                    "Portal shortcut active (registered via org.freedesktop.portal.GlobalShortcuts)"
                        .to_string(),
            },
        };
        let formatted = format_shortcut_status(&status);
        assert!(formatted.contains("Configured Shortcut : Ctrl+Shift+P"));
        assert!(formatted.contains("Effective Shortcut  : Meta+V"));
        assert!(formatted.contains("Backend             : KDE Plasma GlobalShortcuts Portal"));
        assert!(
            formatted.contains(
                "Capability          : Desktop portal global shortcuts (e.g. KDE Plasma)"
            )
        );
    }

    #[test]
    fn formats_compositor_managed_status() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("Sway IPC Backend".to_string()),
            capability: Some(IpcShortcutCapability::CompositorManaged),
            effective_shortcut: None,
            state: IpcShortcutState::CompositorManaged {
                binding_status: IpcCompositorBindingStatus::Conflict,
                snippet: "bindsym $mod+v exec pookie-paste --toggle".to_string(),
                conflict: Some("Existing binding found for $mod+v".to_string()),
                diagnostic: Some("Found 1 conflict in ~/.config/sway/config".to_string()),
            },
        };
        let formatted = format_shortcut_status(&status);
        assert!(formatted.contains("Status              : Conflict"));
        assert!(
            formatted.contains("Binding Directive   : bindsym $mod+v exec pookie-paste --toggle")
        );
        assert!(formatted.contains("Conflict Detected   : Existing binding found for $mod+v"));
        assert!(
            formatted.contains("Diagnostic          : Found 1 conflict in ~/.config/sway/config")
        );
        assert!(formatted.contains("Action Required     : Resolve the conflicting shortcut"));
    }

    #[test]
    fn formats_compositor_managed_bound_unverified_status() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("Hyprland compositor-managed shortcut".to_string()),
            capability: Some(IpcShortcutCapability::CompositorManaged),
            effective_shortcut: None,
            state: IpcShortcutState::CompositorManaged {
                binding_status: IpcCompositorBindingStatus::BoundUnverified,
                snippet: "hl.bind(\"SUPER + V\", hl.dsp.exec_cmd(\"pookie-paste --toggle\"))"
                    .to_string(),
                conflict: None,
                diagnostic: Some(
                    "Key is actively bound to a Lua callback (__lua, id: 99) in Hyprland; runtime command target cannot be verified over IPC".to_string(),
                ),
            },
        };
        let formatted = format_shortcut_status(&status);
        assert!(formatted.contains("Status              : Bound / Unverified"));
        assert!(formatted.contains("Binding Directive   : hl.bind(\"SUPER + V\""));
        assert!(formatted.contains(
            "Diagnostic          : Key is actively bound to a Lua callback (__lua, id: 99)"
        ));
        assert!(formatted.contains(
            "Guidance            : Ensure the active binding executes 'pookie-paste --toggle'"
        ));
        assert!(!formatted.contains("Action Required"));
    }
}
