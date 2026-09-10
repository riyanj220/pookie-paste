mod clipboard_backend;
mod clipboard_reader;

mod mime;

mod watcher;

mod protocol;
mod registry;

mod ext_backend;
mod wlr_backend;

mod wlr_data_control;

mod ext_data_control;
mod ext_protocol;

pub use clipboard_backend::WaylandClipboard;

pub use watcher::WaylandClipboardWatcher;

pub use protocol::WaylandClipboardProtocol;
