//! An ordered run of bare keys that the caller acts on when it completes.

use std::time::Duration;

use freddie_keys::{Key, KeyEvent, KeyPress, PressType};

use crate::TimerGuard;

/// A run of keys that means something other than what it types: `jk`, say.
///
/// Each key is swallowed until the run breaks (swallowed keys replay) or completes
/// (they are dropped and the caller acts). Any modifier flag breaks it. Keys may
/// overlap: the next may go down before the previous comes up.
pub struct KeySequence {
    keys: &'static [Key],
    /// How long a run waits for its next key, and the guard while one is live.
    window: Option<Window>,
    /// Swallowed presses in arrival order. Empty when idle. Downs count progress;
    /// ups belong to keys already matched.
    swallowed: Vec<KeyPress>,
}

/// How long a run waits for its next key, and what cancels that wait. One field so a
/// guard without a duration cannot exist.
#[derive(Debug)]
struct Window {
    duration: Duration,
    /// Armed while a run is live. Dropping it cancels the wait.
    timer: Option<TimerGuard>,
}

/// What one key did to a [`KeySequence`].
#[derive(Debug, PartialEq, Eq)]
pub enum KeySequenceOutcome {
    /// The key belongs to the run: it was swallowed.
    Advanced,
    /// The key is not part of the run. These presses replay, then the caller emits the key.
    /// Empty when the run was idle.
    Passed(Vec<KeyPress>),
    /// The last key landed. Swallowed keys are dropped; the caller acts.
    Completed,
}

impl std::fmt::Debug for KeySequence {
    /// Keys matched so far: `KeySequence { KeyJ }`, or `KeySequence {}` when idle.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KeySequence {{")?;
        let mut matched = self
            .swallowed
            .iter()
            .filter(|p| p.press == PressType::Down)
            .peekable();
        let any = matched.peek().is_some();
        for (i, press) in matched.enumerate() {
            write!(f, "{}{:?}", if i == 0 { " " } else { ", " }, press.key)?;
        }
        f.write_str(if any { " }" } else { "}" })
    }
}

impl KeySequence {
    /// A sequence of `keys`, idle.
    ///
    /// # Panics
    ///
    /// If `keys` is empty.
    #[must_use]
    pub fn new(keys: &'static [Key], window: Option<Duration>) -> Self {
        assert!(!keys.is_empty(), "a sequence needs at least one key");
        Self {
            keys,
            window: window.map(|duration| Window {
                duration,
                timer: None,
            }),
            swallowed: Vec::new(),
        }
    }

    /// The guard for a live run's window timer, or `None` when idle or this sequence has no window.
    #[must_use]
    pub fn window_timer(&self) -> Option<&TimerGuard> {
        self.window.as_ref()?.timer.as_ref()
    }

    /// How long a run of this sequence waits for its next key, or `None` if it waits forever.
    #[must_use]
    pub fn window(&self) -> Option<Duration> {
        self.window.as_ref().map(|w| w.duration)
    }

    /// Give the live run the guard for its window. Dropping the run cancels the wait.
    ///
    /// # Panics
    ///
    /// If no run is in progress, or if this sequence has no window.
    pub fn hold(&mut self, guard: TimerGuard) {
        assert!(!self.is_idle(), "an idle run has no life to tie a guard to");
        let window = self
            .window
            .as_mut()
            .expect("a sequence with no window cannot have armed one");
        window.timer = Some(guard);
    }

    /// Whether the run has swallowed nothing.
    #[must_use]
    pub const fn is_idle(&self) -> bool {
        self.swallowed.is_empty()
    }

    pub fn advance(&mut self, ev: &KeyEvent) -> KeySequenceOutcome {
        if !ev.flags.is_empty() {
            return KeySequenceOutcome::Passed(self.interrupt());
        }
        let matched = self.matched();
        match ev.press {
            // The key itself must not still be down: a sequence may repeat a key (`[j, j]`),
            // and auto-repeat is a down of a key that never came up.
            PressType::Down if ev.key == self.keys[matched] && !self.is_down(ev.key) => {
                self.swallowed.push(ev.key.down());
                if matched + 1 == self.keys.len() {
                    self.swallowed.clear();
                    self.disarm();
                    KeySequenceOutcome::Completed
                } else {
                    KeySequenceOutcome::Advanced
                }
            }
            PressType::Up if self.is_down(ev.key) => {
                self.swallowed.push(ev.key.up());
                KeySequenceOutcome::Advanced
            }
            _ => KeySequenceOutcome::Passed(self.interrupt()),
        }
    }

    /// End the run and hand back what it swallowed, in arrival order.
    pub fn interrupt(&mut self) -> Vec<KeyPress> {
        self.disarm();
        std::mem::take(&mut self.swallowed)
    }

    fn disarm(&mut self) {
        if let Some(window) = self.window.as_mut() {
            window.timer = None;
        }
    }

    fn matched(&self) -> usize {
        self.swallowed
            .iter()
            .filter(|p| p.press == PressType::Down)
            .count()
    }

    fn is_down(&self, key: Key) -> bool {
        self.swallowed
            .iter()
            .rev()
            .find(|p| p.key == key)
            .is_some_and(|p| p.press == PressType::Down)
    }
}
