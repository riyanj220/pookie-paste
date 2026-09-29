mod cli;
mod ipc_server;
mod logging;
mod shutdown;

use ipc_server::BindOutcome;

use std::sync::Arc;

use tokio::sync::Mutex;

use tracing::{info, warn};

use pookie_core::{ClipboardEvent, ClipboardProcessor};

use history::{ClipboardHistoryService, HistoryConfig};

use storage::{Database, ImageStore};

use daemon::{
    activation_service::ClipboardActivationService, clipboard_service::ClipboardService,
    clipboard_state::ClipboardState,
};

use daemon::focus_service::FocusService;

use daemon::platform::{
    EnvironmentAudit, resolve_clipboard_backend, resolve_focus_backend, resolve_paste_backend,
};

use daemon::shortcut_listener::ShortcutListener;

use daemon::ui_launcher::{UiLaunchOutcome, UiLauncher};

use daemon::app_paths;

use daemon::clipboard_watcher;

use daemon::reload_coordinator::ReloadCoordinator;
use daemon::shortcut_config::ShortcutConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let action = cli::parse_args();
    if action != cli::CliAction::RunDaemon {
        return cli::run_client(action).await;
    }

    logging::init_logging();

    let ipc_listener = match ipc_server::bind()? {
        BindOutcome::Bound(listener) => listener,

        BindOutcome::AlreadyRunning => {
            info!("Pookie Paste is already running; exiting");

            return Ok(());
        }
    };

    let data_directory = app_paths::ensure_data_directory()?;

    let database_path = app_paths::database_path()?;

    info!(
        path = %database_path.display(),
          "using application database"
    );

    let database_url = format!("sqlite://{}", database_path.display(),);

    let database = Database::new(&database_url).await?;

    info!("database initialized");

    let repository = storage::StorageRepository::new(&database);

    let history_config = HistoryConfig::default();

    let image_store = ImageStore::new(data_directory);

    let history_service =
        ClipboardHistoryService::new(repository, history_config).with_image_store(image_store);

    let removed_orphans = history_service.reconcile_image_store().await?;

    if removed_orphans > 0 {
        info!(
            removed = removed_orphans,
            "removed orphan clipboard image files"
        );
    }

    let history_service = Arc::new(history_service);

    let clipboard_state = Arc::new(ClipboardState::default());

    let env_audit = EnvironmentAudit::detect();

    info!(
        session = ?env_audit.session,
        desktop = ?env_audit.desktop,
        "platform capability audit completed"
    );

    let backend = resolve_clipboard_backend(&env_audit)?;

    info!("clipboard backend: {}", backend.name());

    let mut clipboard_events = clipboard_watcher::start(&backend)?;

    let clipboard_service = Arc::new(Mutex::new(ClipboardService::new(
        backend,
        Arc::clone(&clipboard_state),
    )));

    /*
     * Focus initializes first.
     *
     * Wayland direct paste is only safe when
     * Pookie can restore and confirm the
     * original target window before Ctrl+V.
     */
    let focus_backend = resolve_focus_backend(&env_audit)
        .map_err(|error| anyhow::anyhow!("failed to initialize focus backend: {error:?}"))?;

    info!("focus backend: {}", focus_backend.name());

    let allow_wayland_direct = focus_backend.can_restore_focus();

    let paste_backend = resolve_paste_backend(&env_audit, allow_wayland_direct)
        .map_err(|error| anyhow::anyhow!("failed to initialize paste backend: {error:?}"))?;

    info!("paste backend: {}", paste_backend.name());

    let focus_service = FocusService::new(focus_backend);

    if let Err(err) = ShortcutConfig::ensure_config_file_exists() {
        warn!(error = %err, "could not bootstrap configuration file");
    }

    let mut shortcut_listener = ShortcutListener::start();
    let reload_handle = shortcut_listener.reload_handle();
    let reload_coordinator = Arc::new(ReloadCoordinator::new(reload_handle));

    let mut shortcut_available = true;

    let ui_launcher = Arc::new(UiLauncher::new());

    let activation_service = Arc::new(ClipboardActivationService::new(
        Arc::clone(&history_service),
        Arc::clone(&clipboard_service),
        paste_backend,
        focus_service,
    ));

    let processor = ClipboardProcessor::new();

    let shortcut_status = shortcut_listener.status_handle();

    let ipc_future = ipc_server::run(
        ipc_listener,
        Arc::clone(&history_service),
        Arc::clone(&activation_service),
        Arc::clone(&ui_launcher),
        shortcut_status,
        Arc::clone(&reload_coordinator),
    );

    tokio::pin!(ipc_future);

    let mut sighup = shutdown::SighupListener::new()?;

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

                        /*
                         * Both text and image clipboard
                         * writes performed by Pookie produce
                         * normal watcher events.
                         *
                         * Compare the complete canonical
                         * content fingerprint before sending
                         * the event through processing/history.
                         */
                        if clipboard_state
                            .is_self_write(
                                &event.content,
                            )
                            {
                                info!(
                                    "ignoring self-generated clipboard event"
                                );

                                continue;
                            }

                            let core_event =
                            ClipboardEvent {
                                content:
                                event.content,

                                created_at:
                                event.created_at,
                            };

                            if let Some(item) =
                                processor.process(
                                    core_event,
                                )
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

            Some(()) = sighup.recv() => {
                info!("SIGHUP received; initiating configuration reload");
                match reload_coordinator.reload().await {
                    Ok(status) => {
                        info!(
                            configured_shortcut = %status.configured_shortcut,
                            effective_shortcut = ?status.effective_shortcut,
                            "configuration and shortcut reloaded successfully via SIGHUP"
                        );
                    }
                    Err(error) => {
                        warn!(
                            error = %error,
                            "configuration reload via SIGHUP failed; current runtime shortcut and active bindings have been preserved"
                        );
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
                        return Err(
                            error,
                        );
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
                            activation
                            .activation_token
                            .as_deref()
                            {
                                info!(
                                    token_present = true,
                                    "global shortcut activated with Wayland activation context"
                                );

                                let _ = token;
                            } else {
                                info!(
                                    "global shortcut activated"
                                );
                            }

                            match ui_launcher
                            .launch()
                            {
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
                        shortcut_available =
                        false;
                    }
                }
            }
        }
    }

    info!("shutting down Pookie services");

    activation_service.shutdown();

    info!("Pookie daemon stopped");

    Ok(())
}
