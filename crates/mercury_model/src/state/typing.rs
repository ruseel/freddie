use std::time::Duration;

use bind::{Bind, if_not_invalidated};
use freddie::{KeySequence, TimerGuard, timer_effect_and_guard};
use freddie_keys::Key;

#[allow(clippy::wildcard_imports)]
use crate::handlers::*;
use crate::{AnyKey, MercuryEffect, MercuryStruct};

use super::LayerPath;

/// Overlay keymap. Beside the binds it describes.
pub(crate) const OVERLAY: &str = include_str!("overlays/typing.txt");

const JK: &[Key] = &[Key::KeyJ, Key::KeyK];

/// How long a `jk` run waits for the next key. Has to cover a deliberately typed `jk` (down, up, down), not only a roll.
pub const JK_TIMEOUT: Duration = Duration::from_millis(200);

/// Arm the run's window. Delay comes from the sequence.
pub(crate) fn arm_jk_timeout(window: Duration) -> (TimerGuard, MercuryEffect) {
    let (guard, effect) = timer_effect_and_guard(window, ());
    (guard, MercuryEffect::Timer(effect))
}

/// Passthrough layer. `jk` leaves for home.
#[derive(Bind, Debug)]
#[node(parent_path = LayerPath)]
#[binds(MercuryStruct)]
#[bind(
    |path| path.get().jk.window_timer().map(TimerGuard::trigger) => if_not_invalidated(jk_timeout),
    AnyKey => if_not_invalidated(pass_through),
)]
pub struct TypingLayer {
    /// `jk` run. Dropped with the layer, which cancels a pending window. Public for the integration tests.
    pub jk: KeySequence,
}

impl TypingLayer {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            jk: KeySequence::new(JK, Some(JK_TIMEOUT)),
        }
    }
}
