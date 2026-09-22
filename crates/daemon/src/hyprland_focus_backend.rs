use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::focus_backend::{FocusBackend, FocusError, FocusTarget};

const HYPRLAND_IPC_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Debug, Clone)]
pub struct HyprlandFocusBackend {
    socket_path: PathBuf,
}

impl HyprlandFocusBackend {
    /// Discovers Hyprland IPC command socket via `$HYPRLAND_INSTANCE_SIGNATURE`.
    ///
    /// Checks `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock` first,
    /// followed by `/tmp/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`.
    pub fn new() -> Result<Self, FocusError> {
        let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").ok_or(FocusError::Unavailable)?;
        let path = find_hyprland_socket(&sig).ok_or(FocusError::Unavailable)?;
        Ok(Self { socket_path: path })
    }

    /// Creates a HyprlandFocusBackend with an explicit socket path (for deterministic testing).
    pub fn from_path(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn name(&self) -> &'static str {
        "Hyprland focus"
    }

    fn send_command(&self, command: &str) -> Result<String, FocusError> {
        send_ipc_command(&self.socket_path, command)
    }
}

impl FocusBackend for HyprlandFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        let response = self.send_command("j/activewindow")?;
        let address = parse_active_window_address(&response).ok_or(FocusError::Unavailable)?;
        Ok(FocusTarget::hyprland(address))
    }

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError> {
        let FocusTarget::Hyprland(address) = target else {
            return Err(FocusError::Failed(format!(
                "expected Hyprland focus target, got {target}"
            )));
        };

        // Hyprland current dispatcher API uses Lua-style dispatcher syntax.
        // Keep this command isolated here because compositor APIs are version-specific.
        let command = format!("dispatch hl.dsp.focus({{ window = \"address:{address}\" }})");
        let response = self.send_command(&command)?;

        if response.trim().eq_ignore_ascii_case("ok") {
            Ok(())
        } else {
            let error_msg = response.trim();
            let msg = if error_msg.is_empty() {
                "unknown error"
            } else {
                error_msg
            };
            Err(FocusError::Failed(format!(
                "Hyprland focus command failed: {msg}"
            )))
        }
    }

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError> {
        let FocusTarget::Hyprland(address) = target else {
            return Err(FocusError::Failed(format!(
                "expected Hyprland focus target, got {target}"
            )));
        };

        let response = self.send_command("j/activewindow")?;
        let active_address = parse_active_window_address(&response);
        Ok(active_address
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(&address)))
    }
}

/// Locates the Hyprland command socket (.socket.sock) given an instance signature.
fn find_hyprland_socket(sig: &std::ffi::OsStr) -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        let candidate = PathBuf::from(xdg)
            .join("hypr")
            .join(sig)
            .join(".socket.sock");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let fallback = PathBuf::from("/tmp/hypr").join(sig).join(".socket.sock");
    if fallback.exists() {
        return Some(fallback);
    }

    None
}

/// Parses the active window address from Hyprland's JSON response (`j/activewindow`).
pub(crate) fn parse_active_window_address(response: &str) -> Option<String> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let address = value.get("address")?.as_str()?;
    let address = address.trim();

    if address.is_empty() || address == "0x" || address == "0x0" {
        return None;
    }

    Some(address.to_string())
}

fn send_ipc_command(socket_path: &Path, command: &str) -> Result<String, FocusError> {
    let mut stream = UnixStream::connect(socket_path).map_err(|err| {
        FocusError::Failed(format!(
            "failed connecting to Hyprland IPC socket {}: {err}",
            socket_path.display()
        ))
    })?;

    stream
        .set_read_timeout(Some(HYPRLAND_IPC_TIMEOUT))
        .map_err(|err| FocusError::Failed(format!("failed setting read timeout: {err}")))?;
    stream
        .set_write_timeout(Some(HYPRLAND_IPC_TIMEOUT))
        .map_err(|err| FocusError::Failed(format!("failed setting write timeout: {err}")))?;

    stream
        .write_all(command.as_bytes())
        .map_err(|err| FocusError::Failed(format!("failed writing Hyprland IPC command: {err}")))?;

    stream
        .flush()
        .map_err(|err| FocusError::Failed(format!("failed flushing Hyprland IPC stream: {err}")))?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|err| {
        FocusError::Failed(format!("failed reading Hyprland IPC response: {err}"))
    })?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_active_window_address() {
        let json_resp = r#"{
            "address": "0x55a72ab3c4d0",
            "mapped": true,
            "hidden": false,
            "class": "kitty",
            "title": "Terminal"
        }"#;

        assert_eq!(
            parse_active_window_address(json_resp),
            Some("0x55a72ab3c4d0".to_string())
        );
    }

    #[test]
    fn parse_returns_none_for_empty_workspace() {
        assert_eq!(parse_active_window_address("{}"), None);
        assert_eq!(parse_active_window_address(""), None);
        assert_eq!(parse_active_window_address("   "), None);
    }

    #[test]
    fn parse_returns_none_for_malformed_json() {
        assert_eq!(parse_active_window_address("{ invalid json"), None);
        assert_eq!(parse_active_window_address("null"), None);
    }

    #[test]
    fn parse_returns_none_for_blank_or_zero_address() {
        assert_eq!(parse_active_window_address(r#"{"address": ""}"#), None);
        assert_eq!(parse_active_window_address(r#"{"address": "   "}"#), None);
        assert_eq!(parse_active_window_address(r#"{"address": "0x"}"#), None);
        assert_eq!(parse_active_window_address(r#"{"address": "0x0"}"#), None);
    }

    #[test]
    fn restore_rejects_non_hyprland_target() {
        let backend = HyprlandFocusBackend::from_path(PathBuf::from("/tmp/nonexistent-hypr.sock"));
        let x11_target = FocusTarget::x11(12345);
        let result = backend.restore(x11_target);
        assert!(result.is_err());
        match result {
            Err(FocusError::Failed(msg)) => {
                assert!(msg.contains("expected Hyprland focus target"))
            }
            other => panic!("expected FocusError::Failed, got {other:?}"),
        }
    }

    #[test]
    fn is_active_rejects_non_hyprland_target() {
        let backend = HyprlandFocusBackend::from_path(PathBuf::from("/tmp/nonexistent-hypr.sock"));
        let sway_target = FocusTarget::sway(42);
        let result = backend.is_active(sway_target);
        assert!(result.is_err());
        match result {
            Err(FocusError::Failed(msg)) => {
                assert!(msg.contains("expected Hyprland focus target"))
            }
            other => panic!("expected FocusError::Failed, got {other:?}"),
        }
    }
}
