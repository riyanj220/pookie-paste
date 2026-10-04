mod error;
mod mapper;

pub mod config;
pub mod notifier;
pub mod service;

pub use config::HistoryConfig;
pub use error::{CandidateDecodeError, HistoryError};
pub use notifier::HistoryRevisionNotifier;
pub use service::{ClipboardHistoryService, HistorySaveOutcome, MigrationSummary};
