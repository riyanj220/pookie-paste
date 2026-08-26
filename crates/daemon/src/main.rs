mod app;
mod config;
mod logging;
mod shutdown;

use app::App;
use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init_logging();

    tracing::info!("Starting Pookie daemon");

    let config = Config::default();

    let app = App::new(config);

    let daemon_task = tokio::spawn(async move {
        app.run().await;
    });

    shutdown::wait_for_shutdown().await?;

    daemon_task.abort();

    tracing::info!("Pookie daemon stopped");

    Ok(())
}
