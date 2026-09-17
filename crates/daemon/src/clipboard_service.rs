use std::sync::Arc;

use anyhow::Result;

use pookie_clipboard::{ClipboardBackend, ClipboardContent};

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

    pub fn read(&self) -> Result<ClipboardContent> {
        let content = self.backend.read_content()?;

        Ok(content)
    }

    pub fn write<C>(&mut self, content: C) -> Result<()>
    where
        C: Into<ClipboardContent>,
    {
        let content = content.into();

        self.backend.write_content(&content)?;

        /*
         * Phase 10.7 will generalize self-write
         * suppression to both text and images using
         * content fingerprints.
         *
         * For Phase 10.4 we intentionally preserve the
         * existing proven text behavior unchanged.
         */
        if let ClipboardContent::Text(text) = &content {
            self.clipboard_state.mark_written(text.clone());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use pookie_clipboard::{ClipboardBackend, ClipboardContent, ClipboardError};

    use crate::clipboard_state::ClipboardState;

    use super::ClipboardService;

    #[derive(Clone)]
    struct FakeClipboardBackend {
        text: Arc<StdMutex<String>>,

        image: Arc<StdMutex<Option<Vec<u8>>>>,
    }

    impl FakeClipboardBackend {
        fn new(initial: &str) -> Self {
            Self {
                text: Arc::new(StdMutex::new(initial.to_string())),

                image: Arc::new(StdMutex::new(None)),
            }
        }

        fn text_content(&self) -> String {
            self.text
                .lock()
                .expect("fake clipboard mutex poisoned")
                .clone()
        }

        fn image_content(&self) -> Option<Vec<u8>> {
            self.image
                .lock()
                .expect("fake clipboard mutex poisoned")
                .clone()
        }
    }

    impl ClipboardBackend for FakeClipboardBackend {
        fn read(&self) -> Result<String, ClipboardError> {
            Ok(self
                .text
                .lock()
                .expect("fake clipboard mutex poisoned")
                .clone())
        }

        fn write(&self, content: &str) -> Result<(), ClipboardError> {
            *self.text.lock().expect("fake clipboard mutex poisoned") = content.to_string();

            Ok(())
        }

        fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
            match content {
                ClipboardContent::Text(text) => self.write(text),

                ClipboardContent::Image(image) => {
                    *self.image.lock().expect("fake clipboard mutex poisoned") =
                        Some(image.clone());

                    Ok(())
                }
            }
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

        assert_eq!(content, ClipboardContent::Text("hello".to_string(),),);
    }

    #[test]
    fn write_text_updates_clipboard_content() {
        let backend = FakeClipboardBackend::new("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        service.write("hello").expect("clipboard write failed");

        assert_eq!(backend_handle.text_content(), "hello",);
    }

    #[test]
    fn write_owned_text_updates_clipboard_content() {
        let backend = FakeClipboardBackend::new("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        service
            .write(String::from("owned text"))
            .expect("clipboard write failed");

        assert_eq!(backend_handle.text_content(), "owned text",);
    }

    #[test]
    fn write_content_text_updates_clipboard_content() {
        let backend = FakeClipboardBackend::new("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        service
            .write(ClipboardContent::Text("content-aware".to_string()))
            .expect("clipboard write failed");

        assert_eq!(backend_handle.text_content(), "content-aware",);
    }

    #[test]
    fn write_image_passes_image_to_content_aware_backend() {
        let backend = FakeClipboardBackend::new("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        let image = vec![1, 2, 3, 4];

        service
            .write(ClipboardContent::Image(image.clone()))
            .expect("clipboard image write failed");

        assert_eq!(backend_handle.image_content(), Some(image),);
    }

    #[test]
    fn write_marks_self_generated_text_content() {
        let backend = FakeClipboardBackend::new("");

        let state = Arc::new(ClipboardState::default());

        let mut service = ClipboardService::new(backend, Arc::clone(&state));

        service
            .write("pookie-write")
            .expect("clipboard write failed");

        assert!(state.is_self_write("pookie-write",),);
    }

    #[test]
    fn image_write_does_not_use_text_self_write_marker_yet() {
        let backend = FakeClipboardBackend::new("");

        let state = Arc::new(ClipboardState::default());

        let mut service = ClipboardService::new(backend, Arc::clone(&state));

        service
            .write(ClipboardContent::Image(vec![1, 2, 3]))
            .expect("clipboard image write failed");

        assert!(!state.is_self_write("anything",),);
    }
}
