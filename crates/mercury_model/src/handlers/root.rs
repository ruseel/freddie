//! Root `AnyKey` post: tracking held modifiers, and mouse button tracking.

use freddie_keys::{KeyEvent, MouseButton, MouseButtonEvent, PressType};
use laserbeam::{Completed, CompletesTo};

use crate::MercuryEffect;
use crate::state::{MouseButtonHoldState, MercuryPath};
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

/// Record mouse button down/up. On down: swallow and mark Held.
/// On up: if still Held (no chord happened), replay the button as effect.
/// If Chorded, just release silently.
pub(crate) fn record_mouse_button<'a>(
    ev: &MouseButtonEvent,
    _snap: (),
    p: crate::state::MercuryPath<'a>,
) -> (Vec<crate::MercuryEffect>, Completed<crate::state::MercuryPath<'a>>) {
    let state = match ev.button {
        MouseButton::Back => &mut p.mouse_held.back,
        MouseButton::Forward => &mut p.mouse_held.forward,
    };
    let effects = match ev.press {
        PressType::Down => {
            *state = MouseButtonHoldState::Held;
            Vec::new()
        }
        PressType::Up => {
            let was = *state;
            *state = MouseButtonHoldState::Released;
            if was == MouseButtonHoldState::Held {
                vec![crate::MercuryEffect::MouseButtonTap(ev.button)]
            } else {
                Vec::new()
            }
        }
    };
    (effects, p.complete())
}
