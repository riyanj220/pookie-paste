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
    ShortcutSetup,
    ShortcutStatus,
    ShortcutStatusPorcelain,
    ReloadConfig,
    Help,
    Version,
}

pub fn parse_args() -> CliAction {
    let args: Vec<String> = env::args().skip(1).collect();
    parse_args_from(args)
}

pub fn parse_args_from(args: impl IntoIterator<Item = impl AsRef<str>>) -> CliAction {
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    if args.is_empty() {
        return CliAction::RunDaemon;
    }

    let has_porcelain = args.iter().any(|a| a == "--porcelain");

    if has_porcelain {
        if args.len() == 2
            && ((args[0] == "--shortcut-status" && args[1] == "--porcelain")
                || (args[0] == "--porcelain" && args[1] == "--shortcut-status"))
        {
            return CliAction::ShortcutStatusPorcelain;
        }

        if args.iter().any(|a| {
            matches!(
                a.as_str(),
                "--toggle"
                    | "-t"
                    | "--reload"
                    | "-r"
                    | "--shortcut-setup"
                    | "--help"
                    | "-h"
                    | "--version"
                    | "-V"
            )
        }) {
            eprintln!("error: '--porcelain' is only supported with '--shortcut-status'");
            eprintln!();
            print_help();
            process::exit(2);
        }

        if args.len() == 1 && args[0] == "--porcelain" {
            eprintln!("error: '--porcelain' requires '--shortcut-status'");
            eprintln!();
            print_help();
            process::exit(2);
        }

        eprintln!("error: unrecognized argument combination with '--porcelain'");
        eprintln!();
        print_help();
        process::exit(2);
    }

    if args.len() == 1 {
        match args[0].as_str() {
            "--toggle" | "-t" => CliAction::ToggleUi,
            "--shortcut-setup" => CliAction::ShortcutSetup,
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
    } else {
        eprintln!("error: unexpected argument '{}'", args[1]);
        eprintln!();
        print_help();
        process::exit(2);
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
    println!("        --shortcut-setup     Open Pookie Paste directly to shortcut settings");
    println!(
        "        --shortcut-status    Print global shortcut configuration and active runtime status"
    );
    println!(
        "        --porcelain          Produce machine-readable output (used with --shortcut-status)"
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
        CliAction::ShortcutSetup => send_shortcut_setup_request().await,
        CliAction::ShortcutStatus => send_shortcut_status_request(false).await,
        CliAction::ShortcutStatusPorcelain => send_shortcut_status_request(true).await,
        CliAction::ReloadConfig => send_reload_request().await,
    }
}

const DAEMON_STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(2500);
const DAEMON_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

/// Returns true if an IPC connection error indicates the daemon is not running.
pub fn is_daemon_absence_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    )
}

#[cfg(unix)]
fn spawn_detached_daemon() -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let current_exe = env::current_exe()?;
    let mut cmd = Command::new(current_exe);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    cmd.spawn()?;
    Ok(())
}

#[cfg(not(unix))]
fn spawn_detached_daemon() -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    let current_exe = env::current_exe()?;
    Command::new(current_exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

async fn wait_for_daemon_ready(path: &Path) -> Result<IpcClient, String> {
    let deadline = tokio::time::Instant::now() + DAEMON_STARTUP_TIMEOUT;

    loop {
        if let Ok(mut client) = IpcClient::connect(path).await
            && let Ok(IpcResponse::Pong) = client.send(&IpcRequest::Ping).await
        {
            return Ok(client);
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(
                "Error: Pookie Paste failed to become responsive after startup.\n\nRun 'pookie-paste' from a terminal to inspect the startup error."
                    .to_string(),
            );
        }

        tokio::time::sleep(DAEMON_POLL_INTERVAL).await;
    }
}

/// Connects to the daemon's IPC socket, or safely starts the daemon detached
/// if it is absent and waits for it to become ready via IPC Ping/Pong.
pub async fn connect_or_ensure_daemon(path: &Path) -> Result<IpcClient, String> {
    match IpcClient::connect(path).await {
        Ok(client) => Ok(client),
        Err(err) if is_daemon_absence_error(&err) => {
            if let Err(spawn_err) = spawn_detached_daemon() {
                return Err(format!(
                    "Error: Failed to start Pookie Paste background daemon: {spawn_err}"
                ));
            }
            wait_for_daemon_ready(path).await
        }
        Err(err) => Err(format!(
            "Error: Could not connect to Pookie Paste IPC socket at {}: {err}",
            path.display()
        )),
    }
}

pub async fn send_toggle_request() -> anyhow::Result<()> {
    let path = socket_path();
    send_toggle_request_to(&path).await
}

pub async fn send_toggle_request_to(path: &Path) -> anyhow::Result<()> {
    let mut client = match connect_or_ensure_daemon(path).await {
        Ok(client) => client,
        Err(err_msg) => {
            eprintln!("{err_msg}");
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

pub async fn send_shortcut_setup_request() -> anyhow::Result<()> {
    let path = socket_path();
    send_shortcut_setup_request_to(&path).await
}

pub async fn send_shortcut_setup_request_to(path: &Path) -> anyhow::Result<()> {
    let mut client = match connect_or_ensure_daemon(path).await {
        Ok(client) => client,
        Err(err_msg) => {
            eprintln!("{err_msg}");
            process::exit(1);
        }
    };

    match client.send(&IpcRequest::OpenShortcutSetup).await {
        Ok(IpcResponse::UiToggled { launched }) => {
            if !launched {
                println!(
                    "Pookie Paste is already open. Use the gear icon to open shortcut settings."
                );
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
            eprintln!("Failed to send shortcut setup request to daemon: {err:?}");
            process::exit(1);
        }
    }
}

pub async fn send_shortcut_status_request(porcelain: bool) -> anyhow::Result<()> {
    let path = socket_path();
    send_shortcut_status_request_to(&path, porcelain).await
}

pub async fn send_shortcut_status_request_to(path: &Path, porcelain: bool) -> anyhow::Result<()> {
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

    match client.send(&IpcRequest::RecheckShortcutStatus).await {
        Ok(IpcResponse::ShortcutStatus { status }) => {
            if porcelain {
                print!("{}", format_shortcut_status_porcelain(&status));
            } else {
                print!("{}", format_shortcut_status(&status));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingShortcutStatus {
    Ready,
    NeedsSetup,
    Conflict,
    BoundUnverified,
    Unavailable,
}

impl OnboardingShortcutStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NeedsSetup => "needs_setup",
            Self::Conflict => "conflict",
            Self::BoundUnverified => "bound_unverified",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingShortcutResult<'a> {
    pub status: OnboardingShortcutStatus,
    pub shortcut: &'a str,
}

pub fn shortcut_status_to_onboarding_status(
    status: &ShortcutStatusInfo,
) -> OnboardingShortcutResult<'_> {
    match &status.state {
        IpcShortcutState::Active { .. } => {
            let shortcut = status
                .effective_shortcut
                .as_deref()
                .unwrap_or(&status.configured_shortcut);
            OnboardingShortcutResult {
                status: OnboardingShortcutStatus::Ready,
                shortcut,
            }
        }
        IpcShortcutState::CompositorManaged { binding_status, .. } => {
            let onboarding_status = match binding_status {
                IpcCompositorBindingStatus::Verified => OnboardingShortcutStatus::Ready,
                IpcCompositorBindingStatus::BoundUnverified => {
                    OnboardingShortcutStatus::BoundUnverified
                }
                IpcCompositorBindingStatus::Unconfigured => OnboardingShortcutStatus::NeedsSetup,
                IpcCompositorBindingStatus::Conflict => OnboardingShortcutStatus::Conflict,
            };
            OnboardingShortcutResult {
                status: onboarding_status,
                shortcut: &status.configured_shortcut,
            }
        }
        IpcShortcutState::Conflict { .. } => OnboardingShortcutResult {
            status: OnboardingShortcutStatus::Conflict,
            shortcut: &status.configured_shortcut,
        },
        IpcShortcutState::Unavailable { .. }
        | IpcShortcutState::Failed { .. }
        | IpcShortcutState::Initializing => OnboardingShortcutResult {
            status: OnboardingShortcutStatus::Unavailable,
            shortcut: &status.configured_shortcut,
        },
    }
}

pub fn format_shortcut_status_porcelain(status: &ShortcutStatusInfo) -> String {
    let result = shortcut_status_to_onboarding_status(status);
    let sanitized_shortcut: String = result
        .shortcut
        .chars()
        .filter(|&c| c != '\n' && c != '\r')
        .collect();
    format!(
        "status={}\nshortcut={}\n",
        result.status.as_str(),
        sanitized_shortcut
    )
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

    #[test]
    fn classifies_daemon_absence_errors_correctly() {
        use std::io::{Error, ErrorKind};

        assert!(is_daemon_absence_error(&Error::from(ErrorKind::NotFound)));
        assert!(is_daemon_absence_error(&Error::from(
            ErrorKind::ConnectionRefused
        )));

        assert!(!is_daemon_absence_error(&Error::from(
            ErrorKind::PermissionDenied
        )));
        assert!(!is_daemon_absence_error(&Error::from(ErrorKind::AddrInUse)));
        assert!(!is_daemon_absence_error(&Error::from(ErrorKind::TimedOut)));
        assert!(!is_daemon_absence_error(&Error::from(
            ErrorKind::AlreadyExists
        )));
    }

    #[test]
    fn parses_shortcut_setup_flag() {
        assert_eq!(
            parse_args_from(["--shortcut-setup"]),
            CliAction::ShortcutSetup
        );
    }

    #[test]
    fn parses_shortcut_status_porcelain_flag_combinations() {
        assert_eq!(
            parse_args_from(["--shortcut-status", "--porcelain"]),
            CliAction::ShortcutStatusPorcelain
        );
        assert_eq!(
            parse_args_from(["--porcelain", "--shortcut-status"]),
            CliAction::ShortcutStatusPorcelain
        );
    }

    #[test]
    fn maps_active_state_with_effective_shortcut_to_ready() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("KDE Portal".to_string()),
            capability: Some(IpcShortcutCapability::Portal),
            effective_shortcut: Some("Meta+V".to_string()),
            state: IpcShortcutState::Active {
                description: "portal shortcut active".to_string(),
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Ready);
        assert_eq!(result.shortcut, "Meta+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=ready\nshortcut=Meta+V\n"
        );
    }

    #[test]
    fn maps_active_state_without_effective_shortcut_to_ready() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("X11".to_string()),
            capability: Some(IpcShortcutCapability::Native),
            effective_shortcut: None,
            state: IpcShortcutState::Active {
                description: "native shortcut active".to_string(),
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Ready);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=ready\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_compositor_verified_to_ready() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("Sway".to_string()),
            capability: Some(IpcShortcutCapability::CompositorManaged),
            effective_shortcut: None,
            state: IpcShortcutState::CompositorManaged {
                binding_status: IpcCompositorBindingStatus::Verified,
                snippet: "bindsym $mod+v exec pookie-paste".to_string(),
                conflict: None,
                diagnostic: None,
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Ready);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=ready\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_compositor_bound_unverified_to_bound_unverified() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("Hyprland".to_string()),
            capability: Some(IpcShortcutCapability::CompositorManaged),
            effective_shortcut: None,
            state: IpcShortcutState::CompositorManaged {
                binding_status: IpcCompositorBindingStatus::BoundUnverified,
                snippet: "hl.bind(\"SUPER + V\")".to_string(),
                conflict: None,
                diagnostic: None,
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::BoundUnverified);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=bound_unverified\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_compositor_unconfigured_to_needs_setup() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("Sway".to_string()),
            capability: Some(IpcShortcutCapability::CompositorManaged),
            effective_shortcut: None,
            state: IpcShortcutState::CompositorManaged {
                binding_status: IpcCompositorBindingStatus::Unconfigured,
                snippet: "bindsym $mod+v exec pookie-paste".to_string(),
                conflict: None,
                diagnostic: None,
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::NeedsSetup);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=needs_setup\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_compositor_conflict_to_conflict() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("Sway".to_string()),
            capability: Some(IpcShortcutCapability::CompositorManaged),
            effective_shortcut: None,
            state: IpcShortcutState::CompositorManaged {
                binding_status: IpcCompositorBindingStatus::Conflict,
                snippet: "bindsym $mod+v exec pookie-paste".to_string(),
                conflict: Some("existing conflict".to_string()),
                diagnostic: None,
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Conflict);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=conflict\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_native_conflict_to_conflict() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("X11".to_string()),
            capability: Some(IpcShortcutCapability::Native),
            effective_shortcut: None,
            state: IpcShortcutState::Conflict {
                details: "already grabbed".to_string(),
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Conflict);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=conflict\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_unavailable_to_unavailable() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: None,
            capability: Some(IpcShortcutCapability::Unsupported),
            effective_shortcut: None,
            state: IpcShortcutState::Unavailable {
                reason: "no display server".to_string(),
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Unavailable);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=unavailable\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_failed_to_unavailable() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("X11".to_string()),
            capability: Some(IpcShortcutCapability::Native),
            effective_shortcut: None,
            state: IpcShortcutState::Failed {
                error: "io error".to_string(),
            },
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Unavailable);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=unavailable\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn maps_initializing_to_unavailable() {
        let status = ShortcutStatusInfo {
            configured_shortcut: "Super+V".to_string(),
            backend_name: Some("X11".to_string()),
            capability: Some(IpcShortcutCapability::Native),
            effective_shortcut: None,
            state: IpcShortcutState::Initializing,
        };
        let result = shortcut_status_to_onboarding_status(&status);
        assert_eq!(result.status, OnboardingShortcutStatus::Unavailable);
        assert_eq!(result.shortcut, "Super+V");
        assert_eq!(
            format_shortcut_status_porcelain(&status),
            "status=unavailable\nshortcut=Super+V\n"
        );
    }

    #[test]
    fn non_auto_start_actions_retain_distinct_parsing() {
        assert_eq!(
            parse_args_from(["--shortcut-status"]),
            CliAction::ShortcutStatus
        );
        assert_eq!(parse_args_from(["--reload"]), CliAction::ReloadConfig);
        assert_eq!(parse_args_from(["-r"]), CliAction::ReloadConfig);
        assert_eq!(parse_args_from(["--toggle"]), CliAction::ToggleUi);
        assert_eq!(parse_args_from(["-t"]), CliAction::ToggleUi);
    }
}
