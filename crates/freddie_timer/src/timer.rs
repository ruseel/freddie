//! An RAII timer: a [`DropGuard`](crate::DropGuard) the node holds, and an effect the loop schedules.
//! Dropping the guard cancels the timer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::oneshot;

#[cfg(feature = "bind")]
use bind::EventTrigger;

use crate::drop_guard::{DropGuard, drop_guard};

/// Identifies one timer. Stamped on both halves so a firing can be matched against the
/// guard still held.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TimerId(u64);

impl TimerId {
    /// Next id. Atomic because a mutable static has to be `Sync`; `Relaxed` because uniqueness
    /// is the only requirement.
    fn mint() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Guard for one timer. Dropping it cancels the timer.
#[must_use = "dropping the guard cancels the timer immediately"]
#[derive(Debug)]
pub struct TimerGuard {
    id: TimerId,
    // Dropping it wakes the receiver the effect carries.
    #[expect(dead_code)]
    guard: DropGuard,
}

impl TimerGuard {
    /// Trigger matching this timer's firing and no other.
    #[must_use]
    #[cfg(feature = "bind")]
    pub const fn trigger(&self) -> TimerTrigger {
        TimerTrigger(self.id)
    }
}

/// A timer fired. The payload is readable only through [`trigger_if_matching`](Self::trigger_if_matching).
#[derive(Clone, Copy, Debug)]
pub struct TimerFired<P = ()> {
    id: TimerId,
    payload: P,
}

impl<P> TimerFired<P> {
    /// The payload if `guard` is still the one this firing was armed with.
    pub fn trigger_if_matching<'a>(&'a self, guard: Option<&TimerGuard>) -> Option<&'a P> {
        guard
            .is_some_and(|guard| guard.id == self.id)
            .then_some(&self.payload)
    }
}

/// Two firings compare equal under `testing` whatever their ids. A rebuilt expected
/// effect cannot know the id.
#[cfg(feature = "testing")]
impl<P> PartialEq for TimerFired<P> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[cfg(feature = "testing")]
impl<P> Eq for TimerFired<P> {}

/// Matches only the firing of the timer it was built from.
#[cfg(feature = "bind")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TimerTrigger(TimerId);

#[cfg(feature = "bind")]
impl EventTrigger for TimerTrigger {
    type Event = TimerFired;
    fn is_matching(&self, ev: &TimerFired) -> bool {
        self.0 == ev.id
    }
}

/// The scheduling half: a delay, the firing to post, and the cancel channel.
#[derive(Debug)]
pub struct TimerEffect<P = ()> {
    pub delay: Duration,
    pub event: TimerFired<P>,
    pub cancel: oneshot::Receiver<()>,
}

/// Two effects compare as their delay and firing. The cancel receiver is a handle, not data.
#[cfg(feature = "testing")]
impl<P> PartialEq for TimerEffect<P> {
    fn eq(&self, other: &Self) -> bool {
        self.delay == other.delay && self.event == other.event
    }
}

#[cfg(feature = "testing")]
impl<P> Eq for TimerEffect<P> {}

/// Linked guard and firing that completes after `delay`. Dropping the guard cancels the timer.
pub fn timer_effect_and_guard<P>(delay: Duration, payload: P) -> (TimerGuard, TimerEffect<P>) {
    let (guard, receiver) = drop_guard();
    let id = TimerId::mint();
    (
        TimerGuard { id, guard },
        TimerEffect {
            delay,
            event: TimerFired { id, payload },
            cancel: receiver,
        },
    )
}
