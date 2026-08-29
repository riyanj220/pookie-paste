#[derive(Debug, Clone)]
pub struct HistoryConfig {
    pub max_items: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { max_items: 30 }
    }
}
