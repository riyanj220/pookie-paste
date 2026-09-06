use std::sync::Mutex;

#[derive(Default)]
pub struct ClipboardState {
    last_written: Mutex<Option<String>>,
}

impl ClipboardState {
    pub fn mark_written(&self, content: String) {
        let mut value = self
            .last_written
            .lock()
            .expect("clipboard state mutex poisoned");

        *value = Some(content);
    }

    pub fn is_self_write(&self, content: &str) -> bool {
        let mut value = self
            .last_written
            .lock()
            .expect("clipboard state mutex poisoned");

        match value.as_ref() {
            Some(last) if last == content => {
                *value = None;

                true
            }

            _ => false,
        }
    }
}
