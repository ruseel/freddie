//! A guard whose drop cancels a paired async job.

use tokio::sync::oneshot;

/// The cancelling half of a drop pair. Dropping it closes the channel and wakes the
/// paired receiver.
#[must_use = "dropping the guard cancels immediately"]
pub struct DropGuard(
    // Dropping the sender wakes the paired receiver.
    #[expect(dead_code)] oneshot::Sender<()>,
);

impl std::fmt::Debug for DropGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DropGuard")
    }
}

/// Linked guard/receiver pair. The guard goes in the node; the receiver rides the effect.
pub fn drop_guard() -> (DropGuard, oneshot::Receiver<()>) {
    let (sender, receiver) = oneshot::channel();
    (DropGuard(sender), receiver)
}
