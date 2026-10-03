use tokio::sync::watch;

/// Thread-safe, in-memory revision notifier for clipboard history mutations.
///
/// Uses `tokio::sync::watch::Sender<u64>` as the single authoritative source of truth.
/// Revisions are strictly positive and monotonically increasing.
#[derive(Clone, Debug)]
pub struct HistoryRevisionNotifier {
    sender: watch::Sender<u64>,
}

impl HistoryRevisionNotifier {
    /// Create a new notifier with an initial revision.
    ///
    /// Revisions are guaranteed to be at least 1.
    pub fn new(initial_revision: u64) -> Self {
        let (sender, _) = watch::channel(initial_revision.max(1));
        Self { sender }
    }

    /// Atomically increments the revision and notifies all active watchers
    /// within the watch channel's internal synchronization boundary.
    pub fn advance(&self) -> u64 {
        self.sender.send_modify(|rev| {
            *rev = rev.wrapping_add(1).max(1);
        });
        *self.sender.borrow()
    }

    /// Returns the current authoritative revision.
    pub fn current(&self) -> u64 {
        *self.sender.borrow()
    }

    /// Subscribe to revision changes.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.sender.subscribe()
    }
}

impl Default for HistoryRevisionNotifier {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_revision_is_at_least_one() {
        let notifier = HistoryRevisionNotifier::new(0);
        assert_eq!(notifier.current(), 1);

        let notifier_default = HistoryRevisionNotifier::default();
        assert_eq!(notifier_default.current(), 1);

        let notifier_custom = HistoryRevisionNotifier::new(42);
        assert_eq!(notifier_custom.current(), 42);
    }

    #[test]
    fn advance_increments_revision_monotonically() {
        let notifier = HistoryRevisionNotifier::new(1);
        assert_eq!(notifier.advance(), 2);
        assert_eq!(notifier.current(), 2);
        assert_eq!(notifier.advance(), 3);
        assert_eq!(notifier.current(), 3);
    }

    #[tokio::test]
    async fn subscriber_receives_advanced_revision() {
        let notifier = HistoryRevisionNotifier::new(1);
        let mut rx = notifier.subscribe();
        assert_eq!(*rx.borrow_and_update(), 1);

        let next = notifier.advance();
        assert_eq!(next, 2);

        assert!(rx.changed().await.is_ok());
        assert_eq!(*rx.borrow_and_update(), 2);
    }

    #[tokio::test]
    async fn multiple_subscribers_wake_simultaneously() {
        let notifier = HistoryRevisionNotifier::new(10);
        let mut rx1 = notifier.subscribe();
        let mut rx2 = notifier.subscribe();

        assert_eq!(*rx1.borrow_and_update(), 10);
        assert_eq!(*rx2.borrow_and_update(), 10);

        notifier.advance();

        assert!(rx1.changed().await.is_ok());
        assert!(rx2.changed().await.is_ok());

        assert_eq!(*rx1.borrow_and_update(), 11);
        assert_eq!(*rx2.borrow_and_update(), 11);
    }
}
