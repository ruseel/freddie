use laserbeam::{Completed, CompletesTo};

use crate::state::{FrontApp, MercuryPath};
use crate::{ForegroundEvent, MercuryEffect};

/// Record the front app. Setting `foreground` ends a pending nav; the in-app level rebuilds from it on the next dispatch.
pub(crate) fn record_front_app<'x>(
    ev: &ForegroundEvent,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    root.foreground = Some(FrontApp::new(ev.app, ev.pid));
    (Vec::new(), root.complete())
}
