//! Overlay: `o` toggles the active layer's keymap; the hide timer takes it down.

use freddie::TimerFired;
use freddie_keys::KeyEvent;
use laserbeam::{Completed, CompletesTo};

use crate::MercuryEffect;
use crate::state::MercuryPath;

/// Toggle the overlay. Typing has no `o` bind, so there an `o` is typed.
pub(crate) fn toggle_overlay<'x>(
    _ev: &KeyEvent,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    let effects = root.toggle_overlay();
    (effects, root.complete())
}

/// Overlay hide timer. Matches the guard the root still holds.
#[expect(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn hide_overlay<'x>(
    _ev: &TimerFired,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    (root.hide_overlay(), root.complete())
}
