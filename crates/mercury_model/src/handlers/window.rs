//! Window source: facts mint and request; value reads commit.

use freddie::TimerFired;
use laserbeam::{Completed, CompletesTo};

use crate::state::MercuryPath;
use crate::{FocusRead, FrameRead, MercuryEffect, WindowEvent};
use freddie_windows_types::WindowChange;

/// Record the change and request the read that fills it. Untracked windows request nothing.
pub(crate) fn record_windows<'x>(
    ev: &WindowEvent,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    let effects = match &ev.change {
        WindowChange::Opened(window) => {
            let (held, riding) = root.generations.mint();
            root.windows.opened(*window, held);
            vec![MercuryEffect::ReadFrame {
                window: *window,
                generation: riding,
            }]
        }
        WindowChange::Moved(window) | WindowChange::Resized(window) => {
            let (held, riding) = root.generations.mint();
            if root.windows.frame_change(*window, held) {
                vec![MercuryEffect::ReadFrame {
                    window: *window,
                    generation: riding,
                }]
            } else {
                Vec::new()
            }
        }
        WindowChange::FocusChanged(pid) => {
            let (held, riding) = root.generations.mint();
            root.windows.focus_change(*pid, held);
            vec![MercuryEffect::ReadFocus {
                pid: *pid,
                generation: riding,
            }]
        }
        WindowChange::Closed(window) => {
            root.windows.closed(*window);
            Vec::new()
        }
        WindowChange::AppGone(pid) => {
            root.windows.app_gone(*pid);
            Vec::new()
        }
        WindowChange::Screens(screens) => {
            root.windows.screens_changed(screens);
            Vec::new()
        }
    };
    (effects, root.complete())
}

pub(crate) fn record_frame_read<'x>(
    ev: &FrameRead,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    root.windows.frame_read(ev.window, &ev.generation, ev.frame);
    (Vec::new(), root.complete())
}

pub(crate) fn record_focus_read<'x>(
    ev: &FocusRead,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    root.windows.focus_read(ev.pid, &ev.generation, ev.window);
    (Vec::new(), root.complete())
}

/// Settle wait ended: further moves are the user's.
#[expect(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn placement_settled<'x>(
    _ev: &TimerFired,
    _snap: (),
    p: MercuryPath<'x>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'x>>) {
    let root: MercuryPath<'x> = p;
    root.windows.forget_pending();
    (Vec::new(), root.complete())
}
