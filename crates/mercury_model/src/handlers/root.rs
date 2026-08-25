//! Root `AnyKey` post: tracking held modifiers.

use freddie_keys::KeyEvent;
use laserbeam::{Completed, CompletesTo};

use crate::MercuryEffect;
use crate::state::MercuryPath;
use bind::AscendState;

/// Keep `held` current. A post so it sees modifiers a deeper binding claimed; the open/close sweeps need that.
pub(crate) fn track_held_modifiers<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, MercuryPath<'x>>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = st.state.into_ancestor();
    if ev.key.is_modifier() {
        root.held.apply(ev);
    }
    (vec![], root.complete())
}
