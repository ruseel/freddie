//! Chrome refresh, address bar, copies, and Ghostty tmux windows.
//!
//! Chrome's `l` is `and!(tap_cmd_l, enter_typing)`: a focused address bar is somewhere you type. Chrome's `r` stays: refreshing repeats.

use freddie_keys::{Key, ModifierFlags};
use laserbeam::{Completed, CompletesTo, HasAncestor, HasStop};

use crate::MercuryEffect;
use crate::effect::{UrlPart, tap};
use crate::sources::host;
use crate::state::{Mercury, MercuryPath};

pub(crate) fn tap_cmd_r<E, P: HasStop + CompletesTo<P>>(
    _ev: &E,
    _snap: (),
    p: P,
) -> (Vec<MercuryEffect>, Completed<P>) {
    (vec![tap(Key::KeyR, ModifierFlags::COMMAND)], p.complete())
}

pub(crate) fn tap_cmd_l<E, P: HasStop + CompletesTo<P>>(
    _ev: &E,
    _snap: (),
    p: P,
) -> (Vec<MercuryEffect>, Completed<P>) {
    (vec![tap(Key::KeyL, ModifierFlags::COMMAND)], p.complete())
}

pub(crate) fn tap_cmd_shift_o<E, P: HasStop + CompletesTo<P>>(
    _ev: &E,
    _snap: (),
    p: P,
) -> (Vec<MercuryEffect>, Completed<P>) {
    (
        vec![tap(
            Key::KeyO,
            ModifierFlags::COMMAND | ModifierFlags::SHIFT,
        )],
        p.complete(),
    )
}

pub(crate) fn copy_url<'a, E, P>(_ev: &E, _snap: (), path: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasAncestor<MercuryPath<'a>> + HasStop + CompletesTo<P>,
{
    let effects = copy(path.ancestor(), UrlPart::Whole);
    (effects, path.complete())
}

pub(crate) fn copy_host<'a, E, P>(_ev: &E, _snap: (), path: P) -> (Vec<MercuryEffect>, Completed<P>)
where
    P: HasAncestor<MercuryPath<'a>> + HasStop + CompletesTo<P>,
{
    let effects = copy(path.ancestor(), UrlPart::Host);
    (effects, path.complete())
}

/// Copy `part` of the front tab's URL. Empty if none is reported, or if `Host` is asked of a URL with no host.
fn copy(root: &Mercury, part: UrlPart) -> Vec<MercuryEffect> {
    let Some(url) = root
        .foreground
        .as_ref()
        .and_then(|front| front.app.chrome())
        .and_then(|chrome| chrome.url.as_deref())
    else {
        return Vec::new();
    };
    let text = match part {
        UrlPart::Whole => Some(url),
        UrlPart::Host => host(url),
    };
    text.map(|text| MercuryEffect::Copy(text.to_owned()))
        .into_iter()
        .collect()
}

/// `ctrl-a` then the command key as two taps. One chord would make tmux see `ctrl-p` rather than `p`.
fn tmux(flags: ModifierFlags, command: Key) -> Vec<MercuryEffect> {
    vec![tap(Key::KeyA, ModifierFlags::CONTROL), tap(command, flags)]
}

/// tmux previous window. Bound alone: walking repeats.
pub(crate) fn tmux_prev<E, P: HasStop + CompletesTo<P>>(
    _ev: &E,
    _snap: (),
    p: P,
) -> (Vec<MercuryEffect>, Completed<P>) {
    (tmux(ModifierFlags::empty(), Key::KeyP), p.complete())
}

pub(crate) fn tmux_next<E, P: HasStop + CompletesTo<P>>(
    _ev: &E,
    _snap: (),
    p: P,
) -> (Vec<MercuryEffect>, Completed<P>) {
    (tmux(ModifierFlags::empty(), Key::KeyN), p.complete())
}

/// Jump to a tmux window. Sends the digit's shifted symbol (`!`..`)`) because that is what the tmux config binds; bare digits cannot reach window 10. Jumping is a choice, so the bind composes `go_home` after this.
pub(crate) fn tmux_window<E, P: HasStop + CompletesTo<P>>(
    digit: Key,
) -> impl Fn(&E, (), P) -> (Vec<MercuryEffect>, Completed<P>) {
    move |_ev, (), p| (tmux(ModifierFlags::SHIFT, digit), p.complete())
}
