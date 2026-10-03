use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::{self, Receiver};
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::SelectionEvent;

use crate::{
    ClipboardContent, ClipboardEvent, ClipboardWatcher,
    x11::{X11Clipboard, read_image_from_arboard, read_text_from_arboard, read_x11_content},
};

#[derive(Clone)]
pub struct X11WatcherShutdownHandle {
    shutdown_write_fd: Arc<std::sync::Mutex<Option<std::os::fd::OwnedFd>>>,
    stopped: Arc<AtomicBool>,
}

impl X11WatcherShutdownHandle {
    pub fn shutdown(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }

        let maybe_fd = self
            .shutdown_write_fd
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());

        if let Some(fd) = maybe_fd {
            use std::os::unix::io::AsRawFd;
            let byte = [1u8];
            unsafe {
                libc::write(fd.as_raw_fd(), byte.as_ptr() as *const libc::c_void, 1);
            }
        }
    }
}

pub struct X11ClipboardWatcher {
    clipboard: Arc<X11Clipboard>,
    shutdown_handle: Option<X11WatcherShutdownHandle>,
    worker_thread: Option<std::thread::JoinHandle<()>>,
}

impl X11ClipboardWatcher {
    pub fn new(clipboard: Arc<X11Clipboard>) -> Self {
        Self {
            clipboard,
            shutdown_handle: None,
            worker_thread: None,
        }
    }

    pub fn shutdown_handle(&self) -> Option<X11WatcherShutdownHandle> {
        self.shutdown_handle.clone()
    }

    pub fn shutdown(&mut self) {
        if let Some(handle) = self.shutdown_handle.take() {
            handle.shutdown();
        }

        if let Some(worker) = self.worker_thread.take() {
            match worker.join() {
                Ok(()) => {}
                Err(error) => {
                    tracing::error!(?error, "failed to join X11 watcher worker thread");
                }
            }
        }
    }
}

impl Drop for X11ClipboardWatcher {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ClipboardWatcher for X11ClipboardWatcher {
    fn start(&mut self) -> Receiver<ClipboardEvent> {
        let (sender, receiver) = mpsc::channel(100);

        let (read_fd, write_fd) = match nix::unistd::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                tracing::error!(error = %error, "failed creating X11 watcher shutdown pipe");
                return receiver;
            }
        };

        let stopped = Arc::new(AtomicBool::new(false));
        let shutdown_write_fd = Arc::new(std::sync::Mutex::new(Some(write_fd)));
        let handle = X11WatcherShutdownHandle {
            shutdown_write_fd,
            stopped: Arc::clone(&stopped),
        };
        self.shutdown_handle = Some(handle);

        let mut target_reader = self.clipboard.take_target_reader();
        if target_reader.is_none() {
            target_reader = crate::x11_targets::X11TargetReader::new().ok();
        }

        let worker = std::thread::Builder::new()
            .name("pookie-x11-watcher".to_string())
            .spawn(move || {
                run_x11_watcher_worker(target_reader, read_fd, sender, stopped);
            });

        match worker {
            Ok(join_handle) => {
                self.worker_thread = Some(join_handle);
            }
            Err(error) => {
                tracing::error!(error = %error, "failed spawning X11 watcher worker thread");
            }
        }

        receiver
    }
}

fn run_x11_watcher_worker(
    mut target_reader: Option<crate::x11_targets::X11TargetReader>,
    shutdown_read_fd: std::os::fd::OwnedFd,
    sender: mpsc::Sender<ClipboardEvent>,
    stopped: Arc<AtomicBool>,
) {
    let mut worker_arboard = match arboard::Clipboard::new() {
        Ok(clip) => clip,
        Err(error) => {
            tracing::error!(
                error = %error,
                "failed initializing watcher-side arboard clipboard"
            );
            return;
        }
    };

    let mut pending_selection_events: VecDeque<Event> = VecDeque::new();

    tracing::info!("starting X11 clipboard watcher worker");

    // 1. Startup: initial clipboard read before entering wait loop.
    // XFixes was already subscribed during target_reader initialization.
    match read_x11_content(
        &mut worker_arboard,
        target_reader.as_mut(),
        &mut pending_selection_events,
    ) {
        Ok(content) if !content_is_empty(&content) => {
            let _ = sender.blocking_send(ClipboardEvent::new(content));
        }
        _ => {}
    }

    let mut last_owner = target_reader
        .as_ref()
        .and_then(|r| r.get_selection_owner().ok())
        .unwrap_or(x11rb::NONE);
    let mut last_timestamp: Option<u32> = None;
    if let Some(reader) = target_reader.as_mut() {
        last_timestamp = reader
            .read_selection_timestamp(&mut pending_selection_events)
            .ok()
            .flatten();
    }
    let mut last_degraded_hash: Option<String> = None;

    let x11_raw_fd = target_reader.as_ref().map(|r| r.connection_fd());
    let use_xfixes = target_reader
        .as_ref()
        .map(|r| r.xfixes_active())
        .unwrap_or(false);

    loop {
        if stopped.load(Ordering::SeqCst) || sender.is_closed() {
            break;
        }

        // 2. Process pending selection events first (e.g. queued during ConvertSelection)
        if let Some(event) = pending_selection_events.pop_front() {
            if let Event::XfixesSelectionNotify(notify) = event {
                match notify.subtype {
                    SelectionEvent::SET_SELECTION_OWNER => {
                        tracing::debug!(
                            owner = notify.owner,
                            "processing queued SetSelectionOwner notification"
                        );
                        if let Some(reader) = target_reader.as_mut() {
                            reader.notify_clipboard_change();
                        }
                        match read_x11_content(
                            &mut worker_arboard,
                            target_reader.as_mut(),
                            &mut pending_selection_events,
                        ) {
                            Ok(content) if !content_is_empty(&content) => {
                                if sender.blocking_send(ClipboardEvent::new(content)).is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    SelectionEvent::SELECTION_WINDOW_DESTROY
                    | SelectionEvent::SELECTION_CLIENT_CLOSE => {
                        tracing::debug!(
                            subtype = ?notify.subtype,
                            "queued selection owner destroyed/closed; invalidating target cache"
                        );
                        if let Some(reader) = target_reader.as_mut() {
                            reader.invalidate();
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        // 3. Wait for X11 events or shutdown signal
        use std::os::unix::io::AsRawFd;
        let mut poll_fds = [
            libc::pollfd {
                fd: shutdown_read_fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: x11_raw_fd.unwrap_or(-1),
                events: libc::POLLIN,
                revents: 0,
            },
        ];

        // When XFixes is active, timeout_ms is a safety heartbeat to verify channel
        // liveness (sender.is_closed()) if the receiver is dropped without dropping the
        // watcher struct. Crucially, a timeout in XFixes mode performs ZERO clipboard reads,
        // image decodes, hashing, or PNG encodes.
        // When XFixes is inactive, timeout_ms (500ms) drives the fallback owner/TIMESTAMP polling cycle.
        let timeout_ms = if use_xfixes { 1000 } else { 500 };
        let poll_ret = unsafe {
            libc::poll(
                poll_fds.as_mut_ptr(),
                poll_fds.len() as libc::nfds_t,
                timeout_ms,
            )
        };

        if poll_ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            tracing::error!(error = %err, "X11 watcher poll failed");
            break;
        }

        // Check shutdown pipe
        if poll_fds[0].revents & libc::POLLIN != 0 {
            tracing::debug!("X11 watcher shutdown pipe signaled");
            break;
        }

        // Check X11 socket errors
        if poll_fds[1].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            tracing::warn!("X11 watcher connection broken or closed");
            break;
        }

        // 4. Normal XFixes event handling
        if use_xfixes {
            if poll_fds[1].revents & libc::POLLIN != 0 {
                let Some(reader) = target_reader.as_mut() else {
                    break;
                };

                while let Ok(Some(event)) = reader.poll_for_event() {
                    match event {
                        Event::XfixesSelectionNotify(notify)
                            if notify.selection == reader.atoms().clipboard =>
                        {
                            match notify.subtype {
                                SelectionEvent::SET_SELECTION_OWNER => {
                                    tracing::debug!(
                                        owner = notify.owner,
                                        "XFixes SetSelectionOwner received; reading clipboard"
                                    );
                                    reader.notify_clipboard_change();
                                    match read_x11_content(
                                        &mut worker_arboard,
                                        Some(reader),
                                        &mut pending_selection_events,
                                    ) {
                                        Ok(content) if !content_is_empty(&content) => {
                                            if sender
                                                .blocking_send(ClipboardEvent::new(content))
                                                .is_err()
                                            {
                                                return;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                SelectionEvent::SELECTION_WINDOW_DESTROY
                                | SelectionEvent::SELECTION_CLIENT_CLOSE => {
                                    tracing::debug!(
                                        owner = notify.owner,
                                        subtype = ?notify.subtype,
                                        "XFixes SelectionWindowDestroy/ClientClose received; updating watcher state"
                                    );
                                    reader.invalidate();
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else {
            // 5. Fallback polling mode (non-XFixes)
            let Some(reader) = target_reader.as_mut() else {
                let current_result = read_image_from_arboard(&mut worker_arboard)
                    .or_else(|_| read_text_from_arboard(&mut worker_arboard));
                match current_result {
                    Ok(content) if !content_is_empty(&content) => {
                        let hash = degraded_content_hash(&content);
                        if last_degraded_hash.as_deref() != Some(&hash) {
                            last_degraded_hash = Some(hash);
                            if sender.blocking_send(ClipboardEvent::new(content)).is_err() {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            };

            let current_owner = match reader.get_selection_owner() {
                Ok(owner) => owner,
                Err(err) => {
                    tracing::debug!(error = %err, "fallback get_selection_owner failed");
                    continue;
                }
            };

            let owner_changed = current_owner != last_owner;
            let mut should_read = false;
            let mut use_degraded_hash = false;

            if owner_changed {
                last_owner = current_owner;
                last_timestamp = None;
                if current_owner != x11rb::NONE {
                    should_read = true;
                }
            } else if current_owner != x11rb::NONE {
                // Same owner: try TIMESTAMP target conversion
                match reader.read_selection_timestamp(&mut pending_selection_events) {
                    Ok(Some(current_ts)) => {
                        if Some(current_ts) != last_timestamp {
                            last_timestamp = Some(current_ts);
                            should_read = true;
                        }
                    }
                    Ok(None) => {
                        // TIMESTAMP unsupported (non-compliant app).
                        // Fall back to degraded content hash check.
                        should_read = true;
                        use_degraded_hash = true;
                    }
                    Err(err) => {
                        tracing::debug!(error = %err, "fallback read_selection_timestamp failed");
                        should_read = true;
                        use_degraded_hash = true;
                    }
                }
            }

            if should_read {
                match read_x11_content(
                    &mut worker_arboard,
                    Some(reader),
                    &mut pending_selection_events,
                ) {
                    Ok(content) if !content_is_empty(&content) => {
                        if use_degraded_hash {
                            let hash = degraded_content_hash(&content);
                            if last_degraded_hash.as_deref() == Some(&hash) {
                                continue;
                            }
                            last_degraded_hash = Some(hash);
                        } else {
                            last_degraded_hash = None;
                        }

                        if sender.blocking_send(ClipboardEvent::new(content)).is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    tracing::info!("X11 clipboard watcher worker stopped");
}

fn degraded_content_hash(content: &ClipboardContent) -> String {
    match content {
        ClipboardContent::Text(text) => {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        ClipboardContent::Image(image) => image.identity().to_versioned_string(),
    }
}

fn content_is_empty(content: &ClipboardContent) -> bool {
    match content {
        ClipboardContent::Text(text) => text.is_empty(),

        ClipboardContent::Image(image) => image.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::content_is_empty;

    use crate::{CanonicalImage, ClipboardContent, ImageIdentity};

    #[test]
    fn empty_text_is_empty_content() {
        assert!(content_is_empty(&ClipboardContent::Text(String::new(),),));
    }

    #[test]
    fn non_empty_text_is_not_empty_content() {
        assert!(!content_is_empty(&ClipboardContent::Text(
            "hello".to_string(),
        ),));
    }

    #[test]
    fn empty_image_is_empty_content() {
        assert!(content_is_empty(&ClipboardContent::Image(
            CanonicalImage::new(Vec::new(), ImageIdentity::from_bytes([0; 32]))
        ),));
    }

    #[test]
    fn non_empty_image_is_not_empty_content() {
        let canonical = crate::canonicalize_rgba(1, 1, &[255, 0, 0, 255]).expect("canonical image");
        assert!(!content_is_empty(&ClipboardContent::Image(canonical)));
    }
}
