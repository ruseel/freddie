//! freddie: a framework for typed event-to-state machines. Work in progress.

pub mod sequence;

pub use freddie_timer::{
    DropGuard, TimerEffect, TimerFired, TimerGuard, TimerId, TimerTrigger, drop_guard,
    timer_effect_and_guard,
};
pub use sequence::{KeySequence, KeySequenceOutcome};
