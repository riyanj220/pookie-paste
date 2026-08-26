mod app;
mod config;
mod logging;
mod shutdown;


use config::Config;
use tracing::info;


#[tokio::main]
async fn main() -> anyhow::Result<()> {


    logging::init_logging();


    info!("Pookie daemon starting");


    let config = Config::default();


    info!(
        "Maximum history items: {}",
        config.max_history_items
    );


    shutdown::wait_for_shutdown()
        .await?;


    info!("Pookie daemon stopped");


    Ok(())

}