use bind::{Bind, if_not_invalidated};
use freddie::TimerGuard;

#[allow(clippy::wildcard_imports)]
use crate::handlers::*;
use crate::{AnyKey, MercuryEffect, MercuryStruct};

use super::{
    AndReturnHomePath, AppLayer, LayerPath, NavLayer, ResizeLayer, SiteLayer, arm_return_home,
};

/// Wrapper that returns home after [`RETURN_TO_HOME_TIMEOUT`](super::RETURN_TO_HOME_TIMEOUT) idle.
///
/// The deadline post sits here, not on the leaves: a leaf `go_home` happens inside this node's descent, so the post sees the leave. On a leaf it would rearm a layer about to die.
#[derive(Bind, Debug)]
#[node(parent_path = LayerPath)]
#[binds(MercuryStruct)]
#[post(AnyKey => if_not_invalidated(home_deadline))]
#[bind(|path| path.get().guard.trigger() => if_not_invalidated(go_home))]
pub struct AndReturnHome<Next> {
    #[child]
    layers: Next,
    /// Drop cancels the return-home timer.
    pub(crate) guard: TimerGuard,
}

impl<Next> AndReturnHome<Next> {
    #[must_use]
    pub(crate) fn new(layers: impl Into<Next>) -> (Self, MercuryEffect) {
        let (guard, timer) = arm_return_home();
        (
            Self {
                layers: layers.into(),
                guard,
            },
            timer,
        )
    }

    /// Public for the integration tests.
    #[must_use]
    pub const fn layers(&self) -> &Next {
        &self.layers
    }
}

/// Active return-home layer. `From` lets `AndReturnHome::new(NavLayer::new())` construct it.
#[derive(Bind, Debug, derive_more::From)]
#[node(parent_path = AndReturnHomePath)]
#[binds(MercuryStruct)]
pub enum ReturnHomeLayers {
    Nav(NavLayer),
    Resize(ResizeLayer),
    InApp(AppLayer),
    Site(SiteLayer),
}
