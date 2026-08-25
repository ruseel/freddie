//! Return-home deadline.

use freddie_keys::KeyEvent;
use laserbeam::{Completed, CompletesTo, PathMut};

use crate::MercuryEffect;
use crate::state::{AndReturnHome, LayerPath, arm_return_home};

/// On a stay, overwrite the guard (drop cancels the old timer). On a leave, `set_layer` already dropped it.
pub(crate) fn home_deadline<'x, Next: 'static>(
    _ev: &KeyEvent,
    _snap: (),
    mut p: PathMut<AndReturnHome<Next>, LayerPath<'x>>,
) -> (
    Vec<MercuryEffect>,
    Completed<PathMut<AndReturnHome<Next>, LayerPath<'x>>>,
) {
    let (guard, arm) = arm_return_home();
    p.get_mut().guard = guard;
    (vec![arm], p.complete())
}
