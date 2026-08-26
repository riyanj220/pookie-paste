#[derive(Debug)]
pub struct Config {
    pub max_history_items: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_history_items: 100,
        }
    }
}
