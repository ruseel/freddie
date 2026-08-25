//! Dual-role state machines and flag rewrites over [`freddie_keys`].
//!
//! A consumer feeds [`freddie_keys::KeyEvent`]s and gets back events to emit and flags to stamp.
//! Ordered timed chords (`jk`) live in `freddie::KeySequence`.

mod alone_or_modifier;
mod invert_shift;

pub use alone_or_modifier::AloneOrModifier;
pub use invert_shift::invert_shift;
