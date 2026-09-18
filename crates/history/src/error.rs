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
