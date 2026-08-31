#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTarget {
    id: u64,
}

impl FocusTarget {
    pub fn new(id: u64) -> Self {
        Self { id }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

#[derive(Debug)]
pub enum FocusError {
    Unavailable,
    Failed(String),
}

pub trait FocusBackend: Send + Sync {
    fn active_target(&self) -> Result<FocusTarget, FocusError>;

    fn restore(&self, target: FocusTarget) -> Result<(), FocusError>;

    fn is_active(&self, target: FocusTarget) -> Result<bool, FocusError>;
}
