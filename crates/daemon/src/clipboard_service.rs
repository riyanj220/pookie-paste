use std::sync::Arc;

use anyhow::Result;

use pookie_clipboard::ClipboardBackend;

use crate::clipboard_state::ClipboardState;

pub struct ClipboardService<B>
where
    B: ClipboardBackend,
{
    backend: B,

    clipboard_state: Arc<ClipboardState>,
}

impl<B> ClipboardService<B>
where
    B: ClipboardBackend,
{
    pub fn new(backend: B, clipboard_state: Arc<ClipboardState>) -> Self {
        Self {
            backend,
            clipboard_state,
        }
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
         * The watcher will receive this clipboard update,
         * but daemon will ignore this event once.
         */
        self.clipboard_state.mark_written(content.to_string());

        Ok(())
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

        fn content(&self) -> String {
            self.value
                .lock()
                .expect("fake clipboard mutex poisoned")
                .clone()
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
    fn read_returns_current_clipboard_content() {
        let backend = FakeClipboardBackend::new("hello");

        let service = create_service(backend);

        let content = service.read().expect("clipboard read failed");

        assert_eq!(content, "hello");
    }

    #[test]
    fn write_updates_clipboard_content() {
        let backend = FakeClipboardBackend::new("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        service.write("hello").expect("clipboard write failed");

        assert_eq!(backend_handle.content(), "hello");
    }

    #[test]
    fn write_marks_self_generated_clipboard_content() {
        let backend = FakeClipboardBackend::new("");

        let mut service = create_service(backend);

        service
            .write("pookie-write")
            .expect("clipboard write failed");
    }
}
