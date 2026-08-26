use anyhow::Result;

use pookie_clipboard::ClipboardBackend;

pub struct ClipboardService<B>
where
    B: ClipboardBackend,
{
    backend: B,
    last_content: Option<String>,
}

impl<B> ClipboardService<B>
where
    B: ClipboardBackend,
{
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            last_content: None,
        }
    }

    pub fn read(&self) -> Result<String> {
        let content = self.backend.read()?;

        Ok(content)
    }

    pub fn write(&self, content: &str) -> Result<()> {
        self.backend.write(content)?;

        Ok(())
    }

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
