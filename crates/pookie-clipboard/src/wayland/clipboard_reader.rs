use std::{fs::File, io::Read, os::fd::OwnedFd};

use tokio::sync::mpsc::Sender;

use crate::{ClipboardContent, ClipboardEvent};

pub fn read_clipboard_fd(fd: OwnedFd) -> Result<String, String> {
    let mut file = File::from(fd);

    let mut contents = String::new();

    file.read_to_string(&mut contents)
        .map_err(|error| format!("failed reading clipboard fd: {error}"))?;

    Ok(contents)
}

pub fn send_clipboard_event(sender: Sender<ClipboardEvent>, value: String) {
    if value.is_empty() {
        tracing::debug!("ignoring empty clipboard payload");

        return;
    }

    let event = ClipboardEvent {
        id: uuid::Uuid::new_v4().to_string(),

        content: ClipboardContent::Text(value),

        created_at: chrono::Utc::now(),
    };

    if let Err(error) = sender.blocking_send(event) {
        tracing::error!(
            error = %error,
            "failed sending clipboard event"
        );
    }
}
