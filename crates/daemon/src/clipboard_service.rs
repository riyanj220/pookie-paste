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

        /*
         * Only mark the write after the backend has
         * successfully accepted the clipboard content.
         *
         * A failed clipboard write must never leave behind
         * a suppression fingerprint for content that Pookie
         * did not actually place on the clipboard.
         */
        self.backend.write_content(&content)?;

        self.clipboard_state.mark_written(&content);

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
        content: Arc<StdMutex<ClipboardContent>>,

        fail_writes: Arc<StdMutex<bool>>,
    }

    impl FakeClipboardBackend {
        fn new(initial: ClipboardContent) -> Self {
            Self {
                content: Arc::new(StdMutex::new(initial)),

                fail_writes: Arc::new(StdMutex::new(false)),
            }
        }

        fn text(initial: &str) -> Self {
            Self::new(ClipboardContent::Text(initial.to_string()))
        }

        fn content(&self) -> ClipboardContent {
            self.content
                .lock()
                .expect("fake clipboard mutex poisoned")
                .clone()
        }

        fn set_fail_writes(&self, fail: bool) {
            *self
                .fail_writes
                .lock()
                .expect("fake clipboard mutex poisoned") = fail;
        }
    }

    impl ClipboardBackend for FakeClipboardBackend {
        fn read(&self) -> Result<String, ClipboardError> {
            match self.content() {
                ClipboardContent::Text(text) => Ok(text),

                ClipboardContent::Image(_) => Err(ClipboardError::UnsupportedContent(
                    "fake clipboard contains image".to_string(),
                )),
            }
        }

        fn write(&self, content: &str) -> Result<(), ClipboardError> {
            self.write_content(&ClipboardContent::Text(content.to_string()))
        }

        fn read_content(&self) -> Result<ClipboardContent, ClipboardError> {
            Ok(self.content())
        }

        fn write_content(&self, content: &ClipboardContent) -> Result<(), ClipboardError> {
            if *self
                .fail_writes
                .lock()
                .expect("fake clipboard mutex poisoned")
            {
                return Err(ClipboardError::WriteFailed(
                    "simulated write failure".to_string(),
                ));
            }

            *self.content.lock().expect("fake clipboard mutex poisoned") = content.clone();

            Ok(())
        }
    }

    fn create_service(backend: FakeClipboardBackend) -> ClipboardService<FakeClipboardBackend> {
        ClipboardService::new(backend, Arc::new(ClipboardState::default()))
    }

    #[test]
    fn read_returns_current_text_clipboard_content() {
        let backend = FakeClipboardBackend::text("hello");

        let service = create_service(backend);

        let content = service.read().expect("clipboard read failed");

        assert_eq!(content, ClipboardContent::Text("hello".to_string(),),);
    }

    #[test]
    fn read_returns_current_image_clipboard_content() {
        let image = vec![1, 2, 3, 4];

        let backend = FakeClipboardBackend::new(ClipboardContent::Image(image.clone()));

        let service = create_service(backend);

        let content = service.read().expect("clipboard read failed");

        assert_eq!(content, ClipboardContent::Image(image,),);
    }

    #[test]
    fn write_text_updates_clipboard_content() {
        let backend = FakeClipboardBackend::text("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        service.write("hello").expect("clipboard write failed");

        assert_eq!(
            backend_handle.content(),
            ClipboardContent::Text("hello".to_string(),),
        );
    }

    #[test]
    fn write_owned_text_updates_clipboard_content() {
        let backend = FakeClipboardBackend::text("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        service
            .write(String::from("owned text"))
            .expect("clipboard write failed");

        assert_eq!(
            backend_handle.content(),
            ClipboardContent::Text("owned text".to_string(),),
        );
    }

    #[test]
    fn write_image_updates_clipboard_content() {
        let backend = FakeClipboardBackend::text("");

        let backend_handle = backend.clone();

        let mut service = create_service(backend);

        let image = vec![1, 2, 3, 4];

        service
            .write(ClipboardContent::Image(image.clone()))
            .expect("clipboard image write failed");

        assert_eq!(backend_handle.content(), ClipboardContent::Image(image,),);
    }

    #[test]
    fn successful_text_write_marks_self_generated_content() {
        let backend = FakeClipboardBackend::text("");

        let state = Arc::new(ClipboardState::default());

        let mut service = ClipboardService::new(backend, Arc::clone(&state));

        let content = ClipboardContent::Text("pookie-write".to_string());

        service
            .write(content.clone())
            .expect("clipboard write failed");

        assert!(state.is_self_write(&content,));
    }

    #[test]
    fn successful_image_write_marks_self_generated_content() {
        let backend = FakeClipboardBackend::text("");

        let state = Arc::new(ClipboardState::default());

        let mut service = ClipboardService::new(backend, Arc::clone(&state));

        let content = ClipboardContent::Image(vec![1, 2, 3, 4]);

        service
            .write(content.clone())
            .expect("clipboard image write failed");

        assert!(state.is_self_write(&content,));
    }

    #[test]
    fn failed_write_does_not_mark_self_generated_content() {
        let backend = FakeClipboardBackend::text("");

        backend.set_fail_writes(true);

        let state = Arc::new(ClipboardState::default());

        let mut service = ClipboardService::new(backend, Arc::clone(&state));

        let content = ClipboardContent::Image(vec![9, 8, 7]);

        assert!(service.write(content.clone(),).is_err());

        assert!(!state.is_self_write(&content,));
    }
}
