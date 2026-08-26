mod app;
mod logging;
mod shutdown;


use tracing::info;


#[tokio::main]
async fn main() -> anyhow::Result<()> {


    logging::init_logging();


    info!("Pookie daemon starting");


    shutdown::wait_for_shutdown()
        .await?;


    info!("Pookie daemon stopped");


    Ok(())

}