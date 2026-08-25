//! Transitions out of home (and the shared `go_home`).

use laserbeam::{Completed, CompletesTo, HasStop, IntoAncestor};

use crate::MercuryEffect;
use crate::state::{
    AndReturnHome, AppLayer, HomeLayer, MercuryPath, NavLayer, ResizeLayer, SiteLayer, TypingLayer,
};

/// Go home. Typing has to bind `escape` explicitly; a plain escape passes through there.
pub(crate) fn go_home<'a, E, P>(_ev: &E, _snap: (), p: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    let root: MercuryPath<'a> = p.into_ancestor();
    let effects = root.set_layer(HomeLayer::new());
    (effects, root.complete())
}

pub(crate) fn enter_nav<'a, E, P>(_ev: &E, _snap: (), p: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    let root: MercuryPath<'a> = p.into_ancestor();
    let (wrapped, timer) = AndReturnHome::new(NavLayer::new());
    let mut effects = root.set_layer(wrapped);
    effects.push(timer);
    (effects, root.complete())
}

pub(crate) fn enter_typing<'a, E, P>(_ev: &E, _snap: (), p: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    let root: MercuryPath<'a> = p.into_ancestor();
    let effects = root.set_layer(TypingLayer::new());
    (effects, root.complete())
}

pub(crate) fn enter_inapp<'a, E, P>(_ev: &E, _snap: (), p: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    let root: MercuryPath<'a> = p.into_ancestor();
    let (wrapped, timer) = AndReturnHome::new(AppLayer::new());
    let mut effects = root.set_layer(wrapped);
    effects.push(timer);
    (effects, root.complete())
}

pub(crate) fn enter_site<'a, E, P>(_ev: &E, _snap: (), p: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    let root: MercuryPath<'a> = p.into_ancestor();
    let (wrapped, timer) = AndReturnHome::new(SiteLayer::new());
    let mut effects = root.set_layer(wrapped);
    effects.push(timer);
    (effects, root.complete())
}

pub(crate) fn enter_resize<'a, E, P>(_ev: &E, _snap: (), p: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<MercuryPath<'a>>,
    MercuryPath<'a>: CompletesTo<P>,
{
    let root: MercuryPath<'a> = p.into_ancestor();
    let (wrapped, timer) = AndReturnHome::new(ResizeLayer::new());
    let mut effects = root.set_layer(wrapped);
    effects.push(timer);
    (effects, root.complete())
}
