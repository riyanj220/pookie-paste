use std::time::{Duration, Instant};

use crate::focus_backend::{FocusBackend, FocusError, FocusTarget};

const FOCUS_POLL_INTERVAL: Duration = Duration::from_millis(5);

const FOCUS_TIMEOUT: Duration = Duration::from_millis(250);

pub struct FocusService<F>
where
    F: FocusBackend,
{
    backend: F,
    poll_interval: Duration,
    timeout: Duration,
}

impl<F> FocusService<F>
where
    F: FocusBackend,
{
    pub fn new(backend: F) -> Self {
        Self {
            backend,
            poll_interval: FOCUS_POLL_INTERVAL,
            timeout: FOCUS_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_timing(backend: F, poll_interval: Duration, timeout: Duration) -> Self {
        Self {
            backend,
            poll_interval,
            timeout,
        }
    }

    pub async fn restore_and_wait(&self, target: FocusTarget) -> Result<(), FocusError> {
        self.backend.restore(target)?;

        let started = Instant::now();

        loop {
            if self.backend.is_active(target)? {
                return Ok(());
            }

            if started.elapsed() >= self.timeout {
                return Err(FocusError::Failed(format!(
                    "focus restoration timed out for target {}",
                    target.id(),
                )));
            }

            tokio::time::sleep(self.poll_interval).await;
        }
    }

    pub fn capture_target(&self) -> Result<FocusTarget, FocusError> {
        self.backend.active_target()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use super::*;

    struct FakeFocusBackend {
        restore_called: Arc<AtomicBool>,
        checks: Arc<AtomicUsize>,
        active_after: usize,
    }

    impl FakeFocusBackend {
        fn new(active_after: usize) -> Self {
            Self {
                restore_called: Arc::new(AtomicBool::new(false)),
                checks: Arc::new(AtomicUsize::new(0)),
                active_after,
            }
        }
    }

    impl FocusBackend for FakeFocusBackend {
        fn active_target(&self) -> Result<FocusTarget, FocusError> {
            Err(FocusError::Unavailable)
        }

        fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
            self.restore_called.store(true, Ordering::SeqCst);

            Ok(())
        }

        fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
            let check = self.checks.fetch_add(1, Ordering::SeqCst);

            Ok(check >= self.active_after)
        }
    }

    #[tokio::test]
    async fn restore_and_wait_succeeds_when_target_becomes_active() {
        let backend = FakeFocusBackend::new(2);

        let restore_called = Arc::clone(&backend.restore_called);

        let checks = Arc::clone(&backend.checks);

        let service =
            FocusService::with_timing(backend, Duration::from_millis(1), Duration::from_millis(20));

        let target = FocusTarget::new(42);

        let result = service.restore_and_wait(target).await;

        assert!(result.is_ok());

        assert!(restore_called.load(Ordering::SeqCst,));

        assert_eq!(checks.load(Ordering::SeqCst), 3,);
    }

    struct NeverActiveFocusBackend {
        restore_called: Arc<AtomicBool>,
    }

    impl NeverActiveFocusBackend {
        fn new() -> Self {
            Self {
                restore_called: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl FocusBackend for NeverActiveFocusBackend {
        fn active_target(&self) -> Result<FocusTarget, FocusError> {
            Err(FocusError::Unavailable)
        }

        fn restore(&self, _target: FocusTarget) -> Result<(), FocusError> {
            self.restore_called.store(true, Ordering::SeqCst);

            Ok(())
        }

        fn is_active(&self, _target: FocusTarget) -> Result<bool, FocusError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn restore_and_wait_times_out_when_target_never_becomes_active() {
        let backend = NeverActiveFocusBackend::new();

        let restore_called = Arc::clone(&backend.restore_called);

        let service =
            FocusService::with_timing(backend, Duration::from_millis(1), Duration::from_millis(5));

        let target = FocusTarget::new(42);

        let result = service.restore_and_wait(target).await;

        assert!(restore_called.load(Ordering::SeqCst,));

        match result {
            Err(FocusError::Failed(message)) => {
                assert!(message.contains("focus restoration timed out"));
            }

            other => {
                panic!("unexpected result: {other:?}");
            }
        }
    }
}
