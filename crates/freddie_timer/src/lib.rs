//! RAII timers: a guard the node holds and an effect the loop schedules.

pub mod always_equal;
pub mod drop_guard;
pub mod timer;

pub use always_equal::AlwaysEqual;
pub use drop_guard::{DropGuard, drop_guard};
pub use timer::{TimerEffect, TimerFired, TimerGuard, TimerId, timer_effect_and_guard};

#[cfg(feature = "bind")]
pub use timer::TimerTrigger;
