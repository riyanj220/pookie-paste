use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use daemon::focus_backend::{FocusBackend, FocusError, FocusTarget};
use daemon::sway_focus_backend::SwayFocusBackend;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

const MAGIC: &[u8; 6] = b"i3-ipc";

struct MockSwayServer {
    socket_path: PathBuf,
    received_commands: Arc<Mutex<Vec<String>>>,
    _handle: thread::JoinHandle<()>,
}

impl MockSwayServer {
    fn spawn(responses: Vec<(u32, Vec<u8>)>) -> Self {
        let count = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let socket_path = std::env::temp_dir().join(format!(
            "pookie-test-sway-{}-{}.sock",
            std::process::id(),
            count
        ));
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path).expect("failed binding test unix socket");
        let received_commands = Arc::new(Mutex::new(Vec::new()));
        let commands_clone = Arc::clone(&received_commands);

        let handle = thread::spawn(move || {
            for (resp_type, resp_payload) in responses {
                let (mut stream, _) = match listener.accept() {
                    Ok(conn) => conn,
                    Err(_) => break,
                };

                let mut header = [0u8; 14];
                if stream.read_exact(&mut header).is_err() {
                    break;
                }

                assert_eq!(&header[0..6], MAGIC);
                let payload_len = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
                let _req_type = u32::from_le_bytes(header[10..14].try_into().unwrap());

                let mut payload = vec![0u8; payload_len];
                if payload_len > 0 {
                    let _ = stream.read_exact(&mut payload);
                    if let Ok(cmd) = String::from_utf8(payload) {
                        commands_clone.lock().unwrap().push(cmd);
                    }
                }

                // Send back response
                let mut resp_header = Vec::with_capacity(14);
                resp_header.extend_from_slice(MAGIC);
                resp_header.extend_from_slice(&(resp_payload.len() as u32).to_le_bytes());
                resp_header.extend_from_slice(&resp_type.to_le_bytes());

                let _ = stream.write_all(&resp_header);
                let _ = stream.write_all(&resp_payload);
                let _ = stream.flush();
            }
        });

        Self {
            socket_path,
            received_commands,
            _handle: handle,
        }
    }
}

impl Drop for MockSwayServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[test]
fn captures_focused_container_from_tree() {
    let mock_tree = br#"{
        "nodes": [
            {
                "id": 42,
                "focused": true
            }
        ]
    }"#;

    let server = MockSwayServer::spawn(vec![(4, mock_tree.to_vec())]);
    let backend = SwayFocusBackend::from_path(server.socket_path.clone());

    let target = backend.active_target().expect("active target failed");
    assert_eq!(target, FocusTarget::Sway(42));
    assert_eq!(target.sway_id(), Some(42));
}

#[test]
fn restores_focus_to_container_and_verifies_sent_command() {
    let mock_resp = br#"[{"success": true}]"#;
    let server = MockSwayServer::spawn(vec![(0, mock_resp.to_vec())]);
    let backend = SwayFocusBackend::from_path(server.socket_path.clone());

    backend
        .restore(FocusTarget::Sway(42))
        .expect("restore should succeed");

    let commands = server.received_commands.lock().unwrap();
    assert_eq!(commands.as_slice(), &["[con_id=42] focus"]);
}

#[test]
fn verifies_is_active_matches_focused_container() {
    let mock_tree = br#"{
        "nodes": [
            {
                "id": 42,
                "focused": true
            }
        ]
    }"#;

    let server = MockSwayServer::spawn(vec![(4, mock_tree.to_vec()), (4, mock_tree.to_vec())]);
    let backend = SwayFocusBackend::from_path(server.socket_path.clone());

    assert!(backend.is_active(FocusTarget::Sway(42)).unwrap());
    assert!(!backend.is_active(FocusTarget::Sway(99)).unwrap());
}

#[test]
fn returns_unavailable_when_no_focused_container() {
    let mock_tree = br#"{
        "nodes": [
            {
                "id": 42,
                "focused": false
            }
        ]
    }"#;

    let server = MockSwayServer::spawn(vec![(4, mock_tree.to_vec())]);
    let backend = SwayFocusBackend::from_path(server.socket_path.clone());

    let result = backend.active_target();
    assert!(matches!(result, Err(FocusError::Unavailable)));
}

#[test]
fn returns_failed_when_json_is_malformed() {
    let mock_tree = br#"{ invalid json"#;

    let server = MockSwayServer::spawn(vec![(4, mock_tree.to_vec())]);
    let backend = SwayFocusBackend::from_path(server.socket_path.clone());

    let result = backend.active_target();
    assert!(matches!(result, Err(FocusError::Failed(_))));
}

#[test]
fn restore_rejects_wrong_focus_target_variant() {
    let backend = SwayFocusBackend::from_path(PathBuf::from("/tmp/nonexistent.sock"));
    let result = backend.restore(FocusTarget::x11(12345));
    assert!(matches!(result, Err(FocusError::Failed(_))));
}

#[test]
fn is_active_rejects_wrong_focus_target_variant() {
    let backend = SwayFocusBackend::from_path(PathBuf::from("/tmp/nonexistent.sock"));
    let result = backend.is_active(FocusTarget::kde(uuid::Uuid::new_v4()));
    assert!(matches!(result, Err(FocusError::Failed(_))));
}

#[test]
fn new_returns_unavailable_when_swaysock_not_set() {
    let prev = std::env::var_os("SWAYSOCK");
    unsafe { std::env::remove_var("SWAYSOCK") };

    let result = SwayFocusBackend::new();
    assert!(matches!(result, Err(FocusError::Unavailable)));

    if let Some(val) = prev {
        unsafe { std::env::set_var("SWAYSOCK", val) };
    }
}
