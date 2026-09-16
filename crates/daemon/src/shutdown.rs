use anyhow::Result;

#[cfg(unix)]
pub async fn wait_for_shutdown() -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        result =
            tokio::signal::ctrl_c() =>
        {
            result?;
        }

        _ = sigterm.recv() => {}
    }

    Ok(())
}

#[cfg(not(unix))]
pub async fn wait_for_shutdown() -> Result<()> {
    tokio::signal::ctrl_c().await?;

    Ok(())
}
