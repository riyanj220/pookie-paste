use std::collections::HashSet;

#[derive(Default)]
pub struct ClipboardHistory {
    hashes: HashSet<String>,
}

impl ClipboardHistory {
    pub fn contains(&self, hash: &str) -> bool {
        self.hashes.contains(hash)
    }

    pub fn insert(&mut self, hash: String) {
        self.hashes.insert(hash);
    }

    pub fn check_and_insert(&mut self, hash: String) -> bool {
        if self.contains(&hash) {
            return false;
        }

        self.insert(hash);

        true
    }
}
