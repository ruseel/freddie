//! What a handler asks the consumer to do.

use freddie::TimerEffect;
use freddie_keys::{Key, KeyEvent, KeyPress, ModifierFlags, MouseButton, PressType};

use freddie_sync::RidingGeneration;
use freddie_windows_types::{Pid, Placement, WindowId};

/// One key with modifiers as flags. A synthetic modifier down/up would strand a modifier the user is really holding.
#[cfg_attr(feature = "testing", derive(PartialEq, Eq))]
#[derive(Clone, Copy, Debug)]
pub struct Chord {
    pub key: Key,
    pub flags: ModifierFlags,
}

/// Which part of a URL a copy puts on the clipboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UrlPart {
    /// The URL as the browser reports it.
    Whole,
    /// The host it names, `www.` and all: `https://www.x.com/asdfasdf` copies `www.x.com`.
    Host,
}

// Equality is only asked for by tests, so the derive is gated. `PartialEq` but not `Eq`: a window event carries `f64` frames.
#[cfg_attr(feature = "testing", derive(PartialEq))]
#[derive(Debug)]
pub enum MercuryEffect {
    Foreground(super::App),
    Tap(Chord),
    /// One press or release. For passthrough, where down and up arrive as separate events. A chord is [`Tap`](Self::Tap).
    Emit(KeyEvent),
    SetFrame(Placement),
    /// Read `window`'s frame; the answer returns as a [`FrameRead`](crate::FrameRead) carrying this half.
    ReadFrame {
        window: WindowId,
        generation: RidingGeneration,
    },
    /// Read `pid`'s focused window; the answer returns as a [`FocusRead`](crate::FocusRead) carrying this half.
    ReadFocus {
        pid: Pid,
        generation: RidingGeneration,
    },
    Copy(String),
    Kill,
    ShowOverlay(&'static str),
    HideOverlay,
    /// Menu-bar layer name. Produced only by `set_layer`, so the item and the model cannot disagree.
    ShowLayer(&'static str),
    /// Replay a mouse side button tap (down+up). Used when the button was held
    /// but no keyboard chord consumed it.
    MouseButtonTap(MouseButton),
    /// Arm a timer. It fires after the delay unless the guard the state kept drops first.
    Timer(TimerEffect),
}

/// So a handler can return one effect or a vec; dispatch collects either.
impl IntoIterator for MercuryEffect {
    type Item = Self;
    type IntoIter = std::iter::Once<Self>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

pub(crate) const fn tap(key: Key, flags: ModifierFlags) -> MercuryEffect {
    MercuryEffect::Tap(Chord { key, flags })
}

pub(crate) const fn emit(key: Key, press: PressType, flags: ModifierFlags) -> MercuryEffect {
    MercuryEffect::Emit(KeyEvent { key, press, flags })
}

/// Replay swallowed presses. They arrived with no modifier, so they go back out bare.
pub(crate) fn replay(presses: Vec<KeyPress>) -> Vec<MercuryEffect> {
    presses
        .into_iter()
        .map(|p| emit(p.key, p.press, ModifierFlags::empty()))
        .collect()
}
