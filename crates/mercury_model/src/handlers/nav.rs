//! App chooser and Spotlight. A choice sets `foreground` to `None` until the watcher reports; the in-app level is empty in the gap.

use freddie_keys::{Key, ModifierFlags};
use laserbeam::{Completed, CompletesTo, HasStop, IntoAncestor};

use crate::effect::tap;
use crate::state::{AndReturnHome, AppLayer, MercuryPath};
use crate::{App, MercuryEffect};

/// Mark nav in flight (`foreground = None`), ask for `app`, enter the in-app layer.
pub(crate) fn open<'a, E, P>(app: App) -> impl Fn(&E, (), P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    move |_ev, _snap, p| {
        let root: MercuryPath<'a> = p.into_ancestor();
        root.foreground = None;
        let mut effects = vec![MercuryEffect::Foreground(app)];
        let (wrapped, timer) = AndReturnHome::new(AppLayer::new());
        effects.extend(root.set_layer(wrapped));
        effects.push(timer);
        (effects, root.complete())
    }
}

/// Ask macOS to launch or foreground an app without entering an app chooser layer.
pub(crate) fn foreground_app<E, P: HasStop + CompletesTo<P>>(
    app: App,
) -> impl Fn(&E, (), P) -> (Vec<MercuryEffect>, Completed<P>) {
    move |_ev, _snap, p| (vec![MercuryEffect::Foreground(app)], p.complete())
}

/// Spotlight's chord. Tap first so the modifier downs from entering typing land on Spotlight.
pub(crate) fn tap_cmd_space<E, P: HasStop + CompletesTo<P>>(
    _ev: &E,
    _snap: (),
    p: P,
) -> (Vec<MercuryEffect>, Completed<P>) {
    (vec![tap(Key::Space, ModifierFlags::COMMAND)], p.complete())
}
