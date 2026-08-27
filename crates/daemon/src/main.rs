mod clipboard_backend;
mod clipboard_service;
mod config;
mod logging;
mod shutdown;

use clipboard_backend::PlatformClipboard;
use clipboard_service::ClipboardService;

use config::Config;

use tokio::time::{Duration, sleep};

use tracing::info;

use pookie_core::{ClipboardEvent, ClipboardProcessor};

use pookie_clipboard::ClipboardContent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init_logging();

    let backend = PlatformClipboard::new()?;

    info!("clipboard backend: {}", backend.name());

    let mut clipboard_service = ClipboardService::new(backend);

    let processor = ClipboardProcessor;

    let config = Config::default();

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
