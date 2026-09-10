mod ipc_server;
mod logging;
mod shutdown;

use std::sync::Arc;

use tokio::sync::Mutex;

use tracing::{info, warn};

use pookie_core::{ClipboardEvent, ClipboardProcessor};

use history::{ClipboardHistoryService, HistoryConfig};

use storage::Database;

use daemon::{
    activation_service::ClipboardActivationService, clipboard_service::ClipboardService,
    clipboard_state::ClipboardState,
};

use daemon::focus_service::FocusService;

use daemon::paste_backend::PlatformPasteBackend;

use daemon::platform_focus_backend::PlatformFocusBackend;

use daemon::shortcut_listener::ShortcutListener;

use daemon::ui_launcher::{UiLaunchOutcome, UiLauncher};

use daemon::clipboard_backend::PlatformClipboard;

use daemon::clipboard_watcher;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init_logging();

    let ipc_listener = ipc_server::bind()?;

    let database = Database::new("sqlite:./pookie-paste.db").await?;

    info!("database initialized");

    let repository = storage::StorageRepository::new(&database);

    let history_config = HistoryConfig::default();

    let history_service = Arc::new(ClipboardHistoryService::new(repository, history_config));

    let clipboard_state = Arc::new(ClipboardState::default());

    let backend = PlatformClipboard::new()?;

    info!("clipboard backend: {}", backend.name());

    let mut clipboard_events = clipboard_watcher::start(&backend)?;

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(
        backend,
        Arc::clone(&clipboard_state),
    )));

    let paste_backend = PlatformPasteBackend::new()
        .map_err(|error| anyhow::anyhow!("failed to initialize paste backend: {error:?}"))?;

    let focus_backend = PlatformFocusBackend::new()
        .map_err(|error| anyhow::anyhow!("failed to initialize focus backend: {error:?}"))?;

    let focus_service = FocusService::new(focus_backend);

    let mut shortcut_listener = ShortcutListener::start();

    let mut shortcut_available = true;

    let ui_launcher = UiLauncher::new();

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
        tokio::select! {
            event =
            clipboard_events.recv() =>
            {
                match event {
                    Some(event) => {
                        info!(
                            "clipboard event received: {}",
                            event.id
                        );

                        let text =
                        match &event.content {
                            pookie_clipboard::ClipboardContent::Text(text) =>
                            Some(text.as_str()),

                            _ => None,
                        };

                        if let Some(text) = text
                            && clipboard_state.is_self_write(text)
                            {
                                info!(
                                    "ignoring self-generated clipboard event"
                                );

                                continue;
                            }

                            let core_event =
                            ClipboardEvent {
                                content: event.content,
                                created_at: event.created_at,
                            };

                            if let Some(item) =
                                processor.process(core_event)
                                {
                                    info!(
                                        "Clipboard item created: {:?}",
                                        item.id
                                    );

                                    history_service
                                    .save(item)
                                    .await?;

                                    info!(
                                        "Clipboard item saved"
                                    );
                                }
                    }

                    None => {
                        warn!(
                            "clipboard watcher stopped"
                        );

                        break;
                    }
                }
            }

            _ =
            shutdown::wait_for_shutdown() =>
            {
                info!(
                    "Shutdown signal received"
                );

                break;
            }

            result =
            &mut ipc_future =>
            {
                match result {
                    Ok(()) => {
                        return Err(
                            anyhow::anyhow!(
                                "IPC server stopped unexpectedly"
                            )
                        );
                    }

                    Err(error) => {
                        return Err(error);
                    }
                }
            }

            activation =
            shortcut_listener.activated(),
            if shortcut_available =>
            {
                match activation {
                    Some(activation) => {
                        if let Some(token) =
                            activation.activation_token.as_deref()
                            {
                                info!(
                                    token_present = true,
                                    "global shortcut activated with Wayland activation context"
                                );

                                /*
                                 * Commit 2:
                                 *
                                 * The Wayland activation token is now
                                 * preserved and propagated to the daemon.
                                 *
                                 * Actual token usage for restoring/focusing
                                 * the target application belongs to the
                                 * next activation commit.
                                 */
                                let _ = token;
                            } else {
                                info!(
                                    "global shortcut activated"
                                );
                            }

                            match ui_launcher.launch() {
                                Ok(
                                    UiLaunchOutcome::Launched
                                )
                                |
                                Ok(
                                    UiLaunchOutcome::AlreadyRunning
                                ) => {}

                                Err(error) => {
                                    warn!(
                                        error = ?error,
                                        "failed to launch Pookie UI"
                                    );
                                }
                            }
                    }

                    None => {
                        shortcut_available = false;
                    }
                }
            }
        }
    }

    info!("Pookie daemon stopped");

    Ok(())
}
