use std::sync::Arc;

use anyhow::Result;

use pookie_clipboard::ClipboardBackend;

use crate::clipboard_state::ClipboardState;

pub struct ClipboardService<B>
where
    B: ClipboardBackend,
{
    backend: B,

    last_content: Option<String>,

    clipboard_state: Arc<ClipboardState>,
}

impl<B> ClipboardService<B>
where
    B: ClipboardBackend,
{
    pub fn new(backend: B, clipboard_state: Arc<ClipboardState>) -> Self {
        Self {
            backend,

            last_content: None,

            clipboard_state,
        }
    }

    pub fn initialize_baseline(&mut self) -> Result<()> {
        let content = self.backend.read()?;

        self.last_content = Some(content);

        Ok(())
    }

    pub fn read(&self) -> Result<String> {
        let content = self.backend.read()?;

        Ok(content)
    }

    pub fn write(&mut self, content: &str) -> Result<()> {
        self.backend.write(content)?;

        /*
         * Mark clipboard writes performed by Pookie.
         *
         * The watcher will see this clipboard update,
         * but daemon will ignore it once.
         */
        self.clipboard_state.mark_written(content.to_string());

        self.last_content = Some(content.to_string());

        Ok(())
    }

    /*
     * Deprecated polling path.
     *
     * Kept temporarily for tests and migration safety.
     * Daemon no longer uses this.
     */
    pub fn check_for_change(&mut self) -> Result<Option<String>> {
        let current = self.backend.read()?;

        let changed = match &self.last_content {
            Some(previous) => previous != &current,

            None => true,
        };

        if changed {
            self.last_content = Some(current.clone());

            return Ok(Some(current));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {

    use std::sync::{Arc, Mutex as StdMutex};

    use pookie_clipboard::{ClipboardBackend, ClipboardError};

    use crate::clipboard_state::ClipboardState;

    use super::ClipboardService;

    #[derive(Clone)]
    struct FakeClipboardBackend {
        value: Arc<StdMutex<String>>,
    }

    impl FakeClipboardBackend {
        fn new(initial: &str) -> Self {
            Self {
                value: Arc::new(StdMutex::new(initial.to_string())),
            }
        }

        fn set_external_value(&self, value: &str) {
            *self.value.lock().expect("fake clipboard mutex poisoned") = value.to_string();
        }
    }

    impl ClipboardBackend for FakeClipboardBackend {
        fn read(&self) -> Result<String, ClipboardError> {
            Ok(self
                .value
                .lock()
                .expect("fake clipboard mutex poisoned")
                .clone())
        }

        fn write(&self, content: &str) -> Result<(), ClipboardError> {
            *self.value.lock().expect("fake clipboard mutex poisoned") = content.to_string();

            Ok(())
        }
    }

    fn create_service(backend: FakeClipboardBackend) -> ClipboardService<FakeClipboardBackend> {
        ClipboardService::new(backend, Arc::new(ClipboardState::default()))
    }

    #[test]
    fn initial_clipboard_is_not_reported_after_baseline_initialization() {
        let backend = FakeClipboardBackend::new("already copied");

        let mut service = create_service(backend);

        service
            .initialize_baseline()
            .expect("baseline initialization failed");

        let change = service.check_for_change().expect("change check failed");

        assert!(change.is_none());
    }

    #[test]
    fn external_change_after_baseline_is_detected() {
        let backend = FakeClipboardBackend::new("A");

        let backend_control = backend.clone();

        let mut service = create_service(backend);

        service
            .initialize_baseline()
            .expect("baseline initialization failed");

        backend_control.set_external_value("B");

        let change = service.check_for_change().expect("change check failed");

        assert_eq!(change.as_deref(), Some("B"));
    }

    #[test]
    fn self_write_is_not_reported_as_new_change() {
        let backend = FakeClipboardBackend::new("");

        let mut service = create_service(backend);

        service.write("B").expect("clipboard write failed");

        let change = service.check_for_change().expect("change check failed");

        assert!(change.is_none());
    }

    #[test]
    fn external_change_after_self_write_is_detected() {
        let backend = FakeClipboardBackend::new("");

        let backend_control = backend.clone();

        let mut service = create_service(backend);

        service.write("B").expect("clipboard write failed");

        backend_control.set_external_value("C");

        let change = service.check_for_change().expect("change check failed");

        assert_eq!(change.as_deref(), Some("C"));
    }
}
