use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::focus_backend::{FocusBackend, FocusError, FocusTarget};

const MAGIC: &[u8; 6] = b"i3-ipc";
const RUN_COMMAND: u32 = 0;
const GET_TREE: u32 = 4;
const SWAY_IPC_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Debug, Clone)]
pub struct SwayFocusBackend {
    socket_path: PathBuf,
}

impl SwayFocusBackend {
    /// Discovers Sway IPC socket via the `$SWAYSOCK` environment variable.
    pub fn new() -> Result<Self, FocusError> {
        let sock = std::env::var_os("SWAYSOCK").ok_or(FocusError::Unavailable)?;
        let path = PathBuf::from(sock);
        if !path.exists() {
            return Err(FocusError::Unavailable);
        }
        Ok(Self { socket_path: path })
    }

    /// Creates a SwayFocusBackend with an explicit socket path (for deterministic testing).
    pub fn from_path(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    fn query_tree(&self) -> Result<serde_json::Value, FocusError> {
        send_ipc_message(&self.socket_path, GET_TREE, b"")
    }

    fn run_command(&self, command: &str) -> Result<serde_json::Value, FocusError> {
        send_ipc_message(&self.socket_path, RUN_COMMAND, command.as_bytes())
    }

    pub fn name(&self) -> &'static str {
        "Sway focus"
    }
}

impl FocusBackend for SwayFocusBackend {
    fn active_target(&self) -> Result<FocusTarget, FocusError> {
        let tree = self.query_tree()?;
        let focused_id = parse_focused_container(&tree).ok_or(FocusError::Unavailable)?;
        Ok(FocusTarget::sway(focused_id))
    }

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError> {
        let FocusTarget::Sway(id) = target else {
            return Err(FocusError::Failed(format!(
                "expected Sway focus target, got {target}"
            )));
        };

        let command = format!("[con_id={id}] focus");
        let response = self.run_command(&command)?;

        // Sway command response is an array of results: [{"success": true}, ...]
        let success = response
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("success"))
            .and_then(|s| s.as_bool())
            .unwrap_or(false);

        if !success {
            let error_msg = response
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("error"))
                .and_then(|err| err.as_str())
                .unwrap_or("unknown error");
            return Err(FocusError::Failed(format!(
                "Sway focus command failed: {error_msg}"
            )));
        }

        Ok(())
    }

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError> {
        let FocusTarget::Sway(id) = target else {
            return Err(FocusError::Failed(format!(
                "expected Sway focus target, got {target}"
            )));
        };

        let tree = self.query_tree()?;
        let focused_id = parse_focused_container(&tree);
        Ok(focused_id == Some(id))
    }
}

/// Recursively traverses a Sway/i3 container tree to locate the node with `"focused": true`.
pub(crate) fn parse_focused_container(node: &serde_json::Value) -> Option<i64> {
    if node
        .get("focused")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        && let Some(id) = node.get("id").and_then(|v| v.as_i64())
    {
        return Some(id);
    }

    if let Some(nodes) = node.get("nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            if let Some(id) = parse_focused_container(child) {
                return Some(id);
            }
        }
    }

    if let Some(floating) = node.get("floating_nodes").and_then(|v| v.as_array()) {
        for child in floating {
            if let Some(id) = parse_focused_container(child) {
                return Some(id);
            }
        }
    }

    None
}

fn send_ipc_message(
    socket_path: &Path,
    msg_type: u32,
    payload: &[u8],
) -> Result<serde_json::Value, FocusError> {
    let mut stream = UnixStream::connect(socket_path).map_err(|err| {
        FocusError::Failed(format!(
            "failed connecting to Sway IPC socket {}: {err}",
            socket_path.display()
        ))
    })?;

    stream
        .set_read_timeout(Some(SWAY_IPC_TIMEOUT))
        .map_err(|err| FocusError::Failed(format!("failed setting read timeout: {err}")))?;
    stream
        .set_write_timeout(Some(SWAY_IPC_TIMEOUT))
        .map_err(|err| FocusError::Failed(format!("failed setting write timeout: {err}")))?;

    let mut header = Vec::with_capacity(14);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    header.extend_from_slice(&msg_type.to_le_bytes());

    stream
        .write_all(&header)
        .map_err(|err| FocusError::Failed(format!("failed writing Sway IPC header: {err}")))?;

    if !payload.is_empty() {
        stream
            .write_all(payload)
            .map_err(|err| FocusError::Failed(format!("failed writing Sway IPC payload: {err}")))?;
    }

    stream
        .flush()
        .map_err(|err| FocusError::Failed(format!("failed flushing Sway IPC stream: {err}")))?;

    let mut response_header = [0u8; 14];
    stream.read_exact(&mut response_header).map_err(|err| {
        FocusError::Failed(format!("failed reading Sway IPC response header: {err}"))
    })?;

    if &response_header[0..6] != MAGIC {
        return Err(FocusError::Failed(
            "invalid Sway IPC response magic".to_string(),
        ));
    }

    let payload_len = u32::from_le_bytes(
        response_header[6..10]
            .try_into()
            .expect("slice with exact size 4"),
    ) as usize;

    let mut payload_buf = vec![0u8; payload_len];
    stream.read_exact(&mut payload_buf).map_err(|err| {
        FocusError::Failed(format!("failed reading Sway IPC response payload: {err}"))
    })?;

    serde_json::from_slice(&payload_buf)
        .map_err(|err| FocusError::Failed(format!("failed parsing Sway IPC response JSON: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_focused_container_from_nested_tree() {
        let tree = json!({
            "id": 1,
            "focused": false,
            "nodes": [
                {
                    "id": 2,
                    "focused": false,
                    "nodes": [
                        {
                            "id": 42,
                            "focused": true,
                            "nodes": []
                        }
                    ]
                },
                {
                    "id": 3,
                    "focused": false,
                    "nodes": []
                }
            ]
        });

        assert_eq!(parse_focused_container(&tree), Some(42));
    }

    #[test]
    fn parses_focused_container_from_floating_nodes() {
        let tree = json!({
            "id": 1,
            "focused": false,
            "nodes": [],
            "floating_nodes": [
                {
                    "id": 99,
                    "focused": true,
                    "nodes": []
                }
            ]
        });

        assert_eq!(parse_focused_container(&tree), Some(99));
    }

    #[test]
    fn returns_none_when_no_node_is_focused() {
        let tree = json!({
            "id": 1,
            "focused": false,
            "nodes": [
                {
                    "id": 2,
                    "focused": false,
                    "nodes": []
                }
            ]
        });

        assert_eq!(parse_focused_container(&tree), None);
    }

    #[test]
    fn restore_rejects_non_sway_target() {
        let backend = SwayFocusBackend::from_path(PathBuf::from("/tmp/nonexistent-sway.sock"));
        let x11_target = FocusTarget::x11(12345);
        let result = backend.restore(x11_target);
        assert!(result.is_err());
        match result {
            Err(FocusError::Failed(msg)) => assert!(msg.contains("expected Sway focus target")),
            other => panic!("expected FocusError::Failed, got {other:?}"),
        }
    }

    #[test]
    fn is_active_rejects_non_sway_target() {
        let backend = SwayFocusBackend::from_path(PathBuf::from("/tmp/nonexistent-sway.sock"));
        let kde_target = FocusTarget::kde(uuid::Uuid::new_v4());
        let result = backend.is_active(kde_target);
        assert!(result.is_err());
        match result {
            Err(FocusError::Failed(msg)) => assert!(msg.contains("expected Sway focus target")),
            other => panic!("expected FocusError::Failed, got {other:?}"),
        }
    }
}
