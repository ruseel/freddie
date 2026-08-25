use bind::{Bind, if_not_invalidated};
use freddie_keys::Key;

use crate::MercuryStruct;
#[allow(clippy::wildcard_imports)]
use crate::handlers::*;

use super::LayerPath;

/// Overlay keymap. Beside the binds it describes.
pub(crate) const OVERLAY: &str = include_str!("overlays/home.txt");

#[derive(Bind, Debug)]
#[node(parent_path = LayerPath)]
#[binds(MercuryStruct)]
#[bind(
    Key::KeyN.down() => if_not_invalidated(enter_nav),
    Key::KeyR.down() => if_not_invalidated(enter_resize),
    Key::KeyT.down() => if_not_invalidated(enter_typing),
    Key::KeyI.down() => if_not_invalidated(enter_inapp),
    Key::KeyU.down() => if_not_invalidated(enter_site),
    Key::KeyQ.down() => if_not_invalidated(quit),
)]
pub struct HomeLayer;

impl HomeLayer {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }
}
