//! Typing's catch-all: the `jk` run, and every other key passed through.

use freddie::{KeySequenceOutcome, TimerFired};
use freddie_keys::KeyEvent;
use laserbeam::{Completed, CompletesTo};

use crate::MercuryEffect;
use crate::effect::{emit, replay};
use crate::state::{HomeLayer, MercuryPath, TypingLayerPath, arm_jk_timeout};

/// Run the key through `jk`. A key the run does not take is passed through with the flags it arrived with.
pub(crate) fn pass_through<'x>(
    ev: &KeyEvent,
    _snap: (),
    mut p: TypingLayerPath<'x>,
) -> (Vec<MercuryEffect>, Completed<TypingLayerPath<'x>>) {
    // Idle before this key iff this key opens the run, which is when the window is armed.
    let opening = p.get().jk.is_idle();
    match p.get_mut().jk.advance(ev) {
        KeySequenceOutcome::Advanced if opening => match p.get().jk.window() {
            Some(window) => {
                let (guard, timer) = arm_jk_timeout(window);
                p.get_mut().jk.hold(guard);
                (vec![timer], p.complete())
            }
            None => (vec![], p.complete()),
        },
        KeySequenceOutcome::Advanced => (vec![], p.complete()),
        KeySequenceOutcome::Passed(presses) => {
            let mut out = replay(presses);
            out.push(emit(ev.key, ev.press, ev.flags));
            (out, p.complete())
        }
        KeySequenceOutcome::Completed => {
            let root: MercuryPath<'x> = p.into_ancestor();
            (root.set_layer(HomeLayer::new()), root.complete())
        }
    }
}

/// Window elapsed: replay what the run swallowed.
#[expect(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn jk_timeout<'x>(
    _ev: &TimerFired,
    _snap: (),
    mut p: TypingLayerPath<'x>,
) -> (Vec<MercuryEffect>, Completed<TypingLayerPath<'x>>) {
    let presses = p.get_mut().jk.interrupt();
    (replay(presses), p.complete())
}
