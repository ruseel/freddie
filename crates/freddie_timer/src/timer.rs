//! An RAII timer.
//!
//! `timer_effect_and_guard` builds a linked pair: a [`DropGuard`](crate::DropGuard) the owning node
//! holds, and an event a handler returns as an effect. The effect loop reads the event's parts and
//! schedules them; dropping the guard cancels the timer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::oneshot;

#[cfg(feature = "bind")]
use bind::EventTrigger;

use crate::drop_guard::{DropGuard, drop_guard};

/// Identifies one timer.
///
/// Minted when the timer is set and stamped on both halves, so a fired event can be matched
/// against the guard still held. Process-wide and monotonic: two timers never share one, whoever
/// set them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TimerId(u64);

impl TimerId {
    /// The next id.
    ///
    /// Atomic because a mutable static has to be `Sync`, not because anything sets timers off one
    /// thread; `Relaxed` because the only requirement is that no two calls return the same value.
    fn mint() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// The guard for one timer: what cancels it, and which timer it is.
///
/// Dropping it cancels the timer, because it drops the [`DropGuard`] inside it. Keeping it is what
/// lets a binding match the event this timer will fire and no other.
#[must_use = "dropping the guard cancels the timer immediately"]
#[derive(Debug)]
pub struct TimerGuard {
    id: TimerId,
    // Held only to be dropped: dropping it wakes the receiver the effect carries. Never read.
    #[expect(dead_code)]
    guard: DropGuard,
}

impl TimerGuard {
    /// The trigger matching this timer's own firing, and no other.
    #[must_use]
    #[cfg(feature = "bind")]
    pub const fn trigger(&self) -> TimerTrigger {
        TimerTrigger(self.id)
    }
}

/// A timer fired, carrying which timer it was and the payload it was armed with.
///
/// `id` and `payload` are private. The payload is readable only through
/// [`trigger_if_matching`](Self::trigger_if_matching).
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

/// Two firings compare equal under `testing` whatever their ids.
///
/// The id exists to tell one timer from another at dispatch. A test that rebuilds an expected
/// effect cannot know it, and asserting it would only assert that the counter ran; with one event
/// type for every timer, the delay is what distinguishes an effect anyway. A test that cares about
/// a firing uses the event on the effect that set the timer, or [`TimerFired::trigger_if_matching`].
#[cfg(feature = "testing")]
impl<P> PartialEq for TimerFired<P> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[cfg(feature = "testing")]
impl<P> Eq for TimerFired<P> {}

/// Matches only the firing of the timer it was built from.
///
/// Its value comes from the guard the bound node holds, so a binding written with it fires for its
/// own timer and nothing else.
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
///
/// A handler returns it as an effect and the effect loop pattern-matches it to schedule. It owns
/// the firing and the receiver, so it is used once. Under `testing`, equality is the delay and the
/// firing; the receiver is a channel handle, not data.
///
/// `P` is the same parameter as [`TimerFired`]. The consumer's event enum is not this type: the
/// effect loop wraps `event` as `Event::Timer(event)` when it posts.
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

/// Build a linked guard and firing that completes after `delay`. Dropping the guard cancels the
/// timer.
///
/// `payload` is stored on the [`TimerFired`] the effect carries. It is readable later only through
/// [`TimerFired::trigger_if_matching`].
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
