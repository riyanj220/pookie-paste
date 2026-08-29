mod clipboard_backend;
mod clipboard_service;
mod config;
mod logging;
mod shutdown;
mod storage_mapper;

use clipboard_backend::PlatformClipboard;
use clipboard_service::ClipboardService;

use config::Config;

use tokio::time::{Duration, sleep};

use tracing::info;

use pookie_core::{ClipboardEvent, ClipboardProcessor};

use pookie_clipboard::ClipboardContent;

use storage::{Database, StorageRepository};
use storage_mapper::to_stored_item;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init_logging();

    let config = Config::default();

    let database = Database::new("sqlite:./pookie-paste.db").await?;

    info!("database initialized");

    let repository = StorageRepository::new(&database);

    info!("storage repository initialized");

    let existing_items = repository.get_all().await?;

    info!(
        "Loaded {} clipboard items from storage",
        existing_items.len()
    );

    let backend = PlatformClipboard::new()?;

    info!("clipboard backend: {}", backend.name());

    let mut clipboard_service = ClipboardService::new(backend);

    let mut processor = ClipboardProcessor::new();

    info!("history limit: {}", config.max_history_items);

    info!("Pookie daemon running");

    loop {
        if let Some(content) = clipboard_service.check_for_change()? {
            let event = ClipboardEvent {
                content: ClipboardContent::Text(content),
                created_at: chrono::Utc::now(),
            };
            if let Some(item) = processor.process(event) {
                info!("Clipboard item created: {:?}", item.id);

                let stored_item = to_stored_item(item);

                repository.insert(&stored_item).await?;

                info!("Clipboard item saved");
            } else {
                info!("Clipboard content ignored");
            }
        }

        tokio::select! {

            _ = shutdown::wait_for_shutdown() => {

                info!("Shutdown signal received");

                break;

            }

            _ = sleep(
                Duration::from_secs(2)
            ) => {}

        }
    }
    info!("Pookie daemon stopped");

    Ok(())
}
