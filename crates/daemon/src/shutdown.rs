use anyhow::Result;

#[cfg(unix)]
pub struct SighupListener {
    signal: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl SighupListener {
    pub fn new() -> Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};

        let signal = signal(SignalKind::hangup())?;
        Ok(Self { signal })
    }

    pub async fn recv(&mut self) -> Option<()> {
        self.signal.recv().await
    }
}

#[cfg(not(unix))]
pub struct SighupListener;

#[cfg(not(unix))]
impl SighupListener {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub async fn recv(&mut self) -> Option<()> {
        std::future::pending().await
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(unix)]
    async fn creates_sighup_listener() {
        assert!(SighupListener::new().is_ok());
    }
}
