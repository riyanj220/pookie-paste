use std::fmt;

#[derive(Debug)]
pub enum ClipboardError {
    InitializationFailed(String),

    ReadFailed(String),

    WriteFailed(String),

    MonitoringFailed(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for ClipboardError {}
