use std::fmt;

#[derive(Debug)]
pub enum ClipboardError {
    InitializationFailed(String),

    ReadFailed(String),

    WriteFailed(String),

    MonitoringFailed(String),

    UnsupportedContent(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitializationFailed(message) => {
                write!(formatter, "clipboard initialization failed: {message}")
            }

            Self::ReadFailed(message) => {
                write!(formatter, "clipboard read failed: {message}")
            }

            Self::WriteFailed(message) => {
                write!(formatter, "clipboard write failed: {message}")
            }

            Self::MonitoringFailed(message) => {
                write!(formatter, "clipboard monitoring failed: {message}")
            }

            Self::UnsupportedContent(message) => {
                write!(formatter, "unsupported clipboard content: {message}")
            }
        }
    }
}

impl std::error::Error for ClipboardError {}
