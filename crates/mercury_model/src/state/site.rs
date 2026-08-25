use bind::{Bind, and, if_not_invalidated};
use freddie_keys::Key;
use laserbeam::HasAncestor;

#[allow(clippy::wildcard_imports)]
use crate::handlers::*;
use crate::{MercuryStruct, Site};

use super::{MercuryPath, ReturnHomeLayersPath, SiteLayerPath};

pub(crate) const OVERLAY: &str = include_str!("overlays/site.txt");
pub(crate) const CLAUDE_AI_OVERLAY: &str = include_str!("overlays/claude-ai.txt");

/// Overlay keymap for the site layer, given the site in the front tab.
pub(crate) const fn overlay_for(site: Option<Site>) -> &'static str {
    match site {
        Some(Site::ClaudeAi) => CLAUDE_AI_OVERLAY,
        Some(Site::Other) | None => OVERLAY,
    }
}

/// Per-tab layer. Stores no site; [`site_data`] reads the front tab URL on every dispatch, so a tab switch changes what is bound.
#[derive(Bind, Debug)]
#[node(parent_path = ReturnHomeLayersPath)]
#[binds(MercuryStruct)]
#[derived_children(site_data)]
pub struct SiteLayer;

impl SiteLayer {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }
}

/// Derived site level. A site with no bindings is not a variant; [`site_data`] returns `None`.
#[derive(Bind, Debug)]
#[derived_node(parent_path = SiteLayerPath)]
#[binds(MercuryStruct)]
pub enum SiteData {
    ClaudeAi(ClaudeAiSite),
}

/// Derived from the front tab URL. `None` if Chrome is not confirmed, no URL yet, or the site has no bindings.
fn site_data<'a, P: HasAncestor<MercuryPath<'a>>>(path: &P) -> Option<SiteData> {
    let root = path.ancestor();
    let url = root
        .foreground
        .as_ref()
        .and_then(|front| front.app.chrome())?
        .url
        .as_deref()?;
    match Site::from_url(url) {
        Site::ClaudeAi => Some(SiteData::ClaudeAi(ClaudeAiSite)),
        Site::Other => None,
    }
}

#[derive(Bind, Debug)]
#[derived_node(parent_path = SiteLayerPath)]
#[binds(MercuryStruct)]
#[bind(Key::KeyN.down() => if_not_invalidated(and!(tap_cmd_shift_o, enter_typing)))]
pub struct ClaudeAiSite;
