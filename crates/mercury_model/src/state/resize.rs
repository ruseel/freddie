use bind::{Bind, if_not_invalidated};
use freddie_keys::Key;

use crate::MercuryStruct;
#[allow(clippy::wildcard_imports)]
use crate::handlers::*;

use super::ReturnHomeLayersPath;

/// Overlay keymap. Beside the binds it describes.
pub(crate) const OVERLAY: &str = include_str!("overlays/resize.txt");

#[derive(Bind, Debug)]
#[node(parent_path = ReturnHomeLayersPath)]
#[binds(MercuryStruct)]
#[bind(
    Key::UpArrow.down() => if_not_invalidated(maximize),
    Key::LeftArrow.down() => if_not_invalidated(left_half),
    Key::RightArrow.down() => if_not_invalidated(right_half),
    Key::KeyR.down() => if_not_invalidated(restore),
)]
pub struct ResizeLayer;

impl ResizeLayer {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }
}
