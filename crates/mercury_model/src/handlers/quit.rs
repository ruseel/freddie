//! Quit, shared by home's `q` and the menu bar.

use laserbeam::{Completed, CompletesTo, HasStop, IntoAncestor};

use crate::MercuryEffect;
use crate::state::MercuryPath;

/// Quit. Emit held-modifier downs first: a command layer swallowed the real downs, and after Kill the grab is released so no further down is coming.
pub(crate) fn quit<'a, E, P>(_ev: &E, _snap: (), p: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    let root: MercuryPath<'a> = p.into_ancestor();
    let mut effects = root.held.open();
    effects.push(MercuryEffect::Kill);
    (effects, root.complete())
}
