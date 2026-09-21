use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use daemon::focus_backend::{FocusBackend, FocusError, FocusTarget};
use daemon::hyprland_focus_backend::HyprlandFocusBackend;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct MockHyprlandServer {
    socket_path: PathBuf,
    received_commands: Arc<Mutex<Vec<String>>>,
    _handle: thread::JoinHandle<()>,
}

impl MockHyprlandServer {
    fn spawn(responses: Vec<String>) -> Self {
        let count = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let socket_path = std::env::temp_dir().join(format!(
            "pookie-test-hypr-{}-{}.sock",
            std::process::id(),
            count
        ));
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path).expect("failed binding test unix socket");
        let received_commands = Arc::new(Mutex::new(Vec::new()));
        let commands_clone = Arc::clone(&received_commands);

        let handle = thread::spawn(move || {
            for resp in responses {
                let (mut stream, _) = match listener.accept() {
                    Ok(conn) => conn,
                    Err(_) => break,
                };

                let mut cmd_buf = [0u8; 1024];
                let bytes_read = match stream.read(&mut cmd_buf) {
                    Ok(n) => n,
                    Err(_) => break,
                };

                let command = String::from_utf8_lossy(&cmd_buf[..bytes_read]).to_string();
                commands_clone.lock().unwrap().push(command);

                // Send back response and drop stream to simulate server closing socket
                let _ = stream.write_all(resp.as_bytes());
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

impl Drop for MockHyprlandServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[test]
fn captures_active_window_address_from_ipc() {
    let mock_resp = r#"{
        "address": "0x55a72ab3c4d0",
        "mapped": true,
        "class": "kitty",
        "title": "Terminal"
    }"#;

    let server = MockHyprlandServer::spawn(vec![mock_resp.to_string()]);
    let backend = HyprlandFocusBackend::from_path(server.socket_path.clone());

    let target = backend.active_target().expect("active target failed");
    assert_eq!(target, FocusTarget::hyprland("0x55a72ab3c4d0"));
    assert_eq!(target.hyprland_address(), Some("0x55a72ab3c4d0"));

    let commands = server.received_commands.lock().unwrap();
    assert_eq!(commands.as_slice(), &["j/activewindow"]);
}

#[test]
fn restores_focus_to_window_and_verifies_sent_command() {
    let server = MockHyprlandServer::spawn(vec!["ok".to_string()]);
    let backend = HyprlandFocusBackend::from_path(server.socket_path.clone());

    backend
        .restore(FocusTarget::hyprland("0x55a72ab3c4d0"))
        .expect("restore should succeed");

    let commands = server.received_commands.lock().unwrap();
    assert_eq!(
        commands.as_slice(),
        &["dispatch focuswindow address:0x55a72ab3c4d0"]
    );
}

#[test]
fn verifies_is_active_matches_address() {
    let mock_resp = r#"{"address": "0x55a72ab3c4d0"}"#;
    let server = MockHyprlandServer::spawn(vec![mock_resp.to_string(), mock_resp.to_string()]);
    let backend = HyprlandFocusBackend::from_path(server.socket_path.clone());

    assert!(
        backend
            .is_active(FocusTarget::hyprland("0x55a72ab3c4d0"))
            .unwrap()
    );
    assert!(
        !backend
            .is_active(FocusTarget::hyprland("0xdeadbeef1234"))
            .unwrap()
    );
}

#[test]
fn returns_unavailable_when_no_active_window() {
    let server = MockHyprlandServer::spawn(vec!["{}".to_string()]);
    let backend = HyprlandFocusBackend::from_path(server.socket_path.clone());

    let result = backend.active_target();
    assert!(matches!(result, Err(FocusError::Unavailable)));
}

#[test]
fn returns_unavailable_when_json_is_malformed() {
    let server = MockHyprlandServer::spawn(vec!["{ invalid json".to_string()]);
    let backend = HyprlandFocusBackend::from_path(server.socket_path.clone());

    let result = backend.active_target();
    assert!(matches!(result, Err(FocusError::Unavailable)));
}

#[test]
fn restore_fails_when_server_returns_error() {
    let server = MockHyprlandServer::spawn(vec!["No such window found".to_string()]);
    let backend = HyprlandFocusBackend::from_path(server.socket_path.clone());

    let result = backend.restore(FocusTarget::hyprland("0x55a72ab3c4d0"));
    assert!(result.is_err());
    match result {
        Err(FocusError::Failed(msg)) => {
            assert!(msg.contains("Hyprland focus command failed: No such window found"));
        }
        other => panic!("expected FocusError::Failed, got {other:?}"),
    }
}

#[test]
fn restore_rejects_wrong_focus_target_variant() {
    let backend = HyprlandFocusBackend::from_path(PathBuf::from("/tmp/nonexistent-hypr.sock"));
    let result = backend.restore(FocusTarget::x11(12345));
    assert!(matches!(result, Err(FocusError::Failed(_))));
}

#[test]
fn is_active_rejects_wrong_focus_target_variant() {
    let backend = HyprlandFocusBackend::from_path(PathBuf::from("/tmp/nonexistent-hypr.sock"));
    let result = backend.is_active(FocusTarget::sway(42));
    assert!(matches!(result, Err(FocusError::Failed(_))));
}

#[test]
fn new_returns_unavailable_when_signature_not_set() {
    let prev = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE");
    unsafe { std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE") };

    let result = HyprlandFocusBackend::new();
    assert!(matches!(result, Err(FocusError::Unavailable)));

    if let Some(val) = prev {
        unsafe { std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", val) };
    }
}
