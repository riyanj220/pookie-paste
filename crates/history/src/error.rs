use std::fmt;

use storage::ImageStoreError;

#[derive(Debug)]
pub enum HistoryError {
    Database(sqlx::Error),

    ImageStore(ImageStoreError),

    ImageRollback {
        database: sqlx::Error,
        cleanup: ImageStoreError,
    },
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "history database error: {error}")
            }

            Self::ImageStore(error) => {
                write!(formatter, "history image storage error: {error}")
            }

            Self::ImageRollback { database, cleanup } => {
                write!(
                    formatter,
                    "history database insert failed ({database}) and image rollback also failed ({cleanup})"
                )
            }
        }
    }
}

impl std::error::Error for HistoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),

            Self::ImageStore(error) => Some(error),

            Self::ImageRollback { database, .. } => Some(database),
        }
    }
}

impl From<sqlx::Error> for HistoryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<ImageStoreError> for HistoryError {
    fn from(error: ImageStoreError) -> Self {
        Self::ImageStore(error)
    }
}

#[derive(Debug)]
pub enum CandidateDecodeError {
    Io(std::io::Error),
    Codec(pookie_clipboard::ImageCodecError),
    MissingFilePath,
    InvalidPath(ImageStoreError),
    IdentityMismatch { expected: String, actual: String },
}

impl fmt::Display for CandidateDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(formatter, "io error reading image file: {err}"),
            Self::Codec(err) => write!(formatter, "image codec error: {err}"),
            Self::MissingFilePath => write!(formatter, "missing image file path"),
            Self::InvalidPath(err) => write!(formatter, "invalid image store path: {err}"),
            Self::IdentityMismatch { expected, actual } => {
                write!(
                    formatter,
                    "image identity mismatch: expected {expected}, actual {actual}"
                )
            }
        }
    }
}

impl std::error::Error for CandidateDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Codec(err) => Some(err),
            Self::InvalidPath(err) => Some(err),
            _ => None,
        }
    }
}
