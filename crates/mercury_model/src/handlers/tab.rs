use laserbeam::{Completed, CompletesTo};

use crate::state::{ForegroundedApp, FrontApp, MercuryPath};
use crate::{MercuryEffect, TabEvent};

/// Record the front tab URL on the confirmed Chrome. Dropped otherwise: a URL mid-nav belongs to the app being left.
pub(crate) fn record_tab_url<'x>(
    ev: &TabEvent,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    if let Some(FrontApp {
        app: ForegroundedApp::Chrome(chrome),
        ..
    }) = &mut root.foreground
    {
        chrome.url = Some(ev.url.clone());
    }
    (Vec::new(), root.complete())
}
