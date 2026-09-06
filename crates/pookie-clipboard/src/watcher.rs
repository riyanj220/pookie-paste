use tokio::sync::mpsc::Receiver;

use crate::ClipboardEvent;

pub trait ClipboardWatcher: Send {
    fn start(&mut self) -> Receiver<ClipboardEvent>;
}
