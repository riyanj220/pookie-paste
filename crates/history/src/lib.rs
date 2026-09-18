mod error;
mod mapper;

pub mod config;
pub mod service;

pub use config::HistoryConfig;
pub use error::HistoryError;
pub use service::ClipboardHistoryService;
