mod clipboard_backend;
mod ipc_server;
mod logging;
mod shutdown;

use std::sync::Arc;

use clipboard_backend::PlatformClipboard;

use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

use tracing::info;

use pookie_core::{ClipboardEvent, ClipboardProcessor};

use pookie_clipboard::ClipboardContent;

use history::{ClipboardHistoryService, HistoryConfig};

use storage::Database;

use daemon::focus_service::FocusService;
use daemon::paste_backend::PlatformPasteBackend;
use daemon::platform_focus_backend::PlatformFocusBackend;
use daemon::shortcut_listener::ShortcutListener;

use daemon::{activation_service::ClipboardActivationService, clipboard_service::ClipboardService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init_logging();

    let ipc_listener = ipc_server::bind()?;

    let database = Database::new("sqlite:./pookie-paste.db").await?;

    info!("database initialized");

    let repository = storage::StorageRepository::new(&database);

    let history_config = HistoryConfig::default();

    info!("history limit: {}", history_config.max_items);

    let history_service = Arc::new(ClipboardHistoryService::new(repository, history_config));

    info!("history service initialized");

    let backend = PlatformClipboard::new()?;

    info!("clipboard backend: {}", backend.name());

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(backend)));

    let paste_backend = PlatformPasteBackend::new()
        .map_err(|error| anyhow::anyhow!("failed to initialize paste backend: {error:?}"))?;

    info!("paste backend: {}", paste_backend.name());

    let focus_backend = PlatformFocusBackend::new()
        .map_err(|error| anyhow::anyhow!("failed to initialize focus backend: {error:?}"))?;

    info!("focus backend: {}", focus_backend.name());

    let focus_service = FocusService::new(focus_backend);

    let mut shortcut_listener = ShortcutListener::start();

    let mut shortcut_available = true;

    let activation_service = Arc::new(ClipboardActivationService::new(
        Arc::clone(&history_service),
        Arc::clone(&clipboard_service),
        paste_backend,
        focus_service,
    ));

    let processor = ClipboardProcessor::new();

    let ipc_future = ipc_server::run(
        ipc_listener,
        Arc::clone(&history_service),
        Arc::clone(&activation_service),
    );

    tokio::pin!(ipc_future);

    info!("Pookie daemon running");

    loop {
        let clipboard_change = {
            let mut clipboard = clipboard_service.lock().await;

            clipboard.check_for_change()?
        };

        if let Some(content) = clipboard_change {
            let event = ClipboardEvent {
                content: ClipboardContent::Text(content),
                created_at: chrono::Utc::now(),
            };

            if let Some(item) = processor.process(event) {
                info!("Clipboard item created: {:?}", item.id);

                history_service.save(item).await?;

                info!("Clipboard item saved");
            } else {
                info!("Clipboard content ignored");
            }
        }

        tokio::select! {
            _ = shutdown::wait_for_shutdown() => {
                info!(
                    "Shutdown signal received"
                );

                break;
            }

            result = &mut ipc_future => {
                match result {
                    Ok(()) => {
                        return Err(
                            anyhow::anyhow!(
                                "IPC server stopped unexpectedly"
                            )
                        );
                    }

                    Err(error) => {
                        return Err(
                            error
                        );
                    }
                }
            }

            activation =
                shortcut_listener.activated(),
                if shortcut_available =>
            {
                match activation {
                    Some(()) => {
                        info!(
                            "global shortcut activated"
                        );
                    }

                    None => {
                        shortcut_available =
                            false;
                    }
                }
            }

            _ = sleep(
                Duration::from_secs(2)
            ) => {}
        }
    }

    info!("Pookie daemon stopped");

    Ok(())
}
