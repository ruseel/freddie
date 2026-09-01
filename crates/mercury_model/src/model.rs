//! The unified trigger, event, and marker the bindings hang off.

use bind::Bindings;
use freddie_keys::{Key, KeyChord, KeyEvent, KeyPress, MouseButtonEvent};

use crate::{
    AnyKey, FocusLanded, FocusRead, ForegroundEvent, Foregrounded, FrameLanded, FrameRead,
    MercuryEffect, MouseButtonPressed, Quit, TabEvent, Tabbed, WindowEvent, Windowed,
};
use freddie::TimerFired;

#[derive(Clone, PartialEq, Eq, Hash, Debug, derive_more::From)]
pub enum MercuryTrigger {
    Key(Key),
    KeyPress(KeyPress),
    KeyChord(KeyChord),
    AnyKey(AnyKey),
    MouseButtonPressed(MouseButtonPressed),
    Foregrounded(Foregrounded),
    Tabbed(Tabbed),
    Windowed(Windowed),
    FrameLanded(FrameLanded),
    FocusLanded(FocusLanded),
    Quit(Quit),
}

/// `PartialEq` but not `Eq` under `testing`: a window frame is four `f64`s.
#[cfg_attr(feature = "testing", derive(PartialEq))]
#[derive(Debug, derive_more::TryInto)]
#[try_into(ref)]
pub enum MercuryEvent {
    Key(KeyEvent),
    MouseButton(MouseButtonEvent),
    Foreground(ForegroundEvent),
    Tab(TabEvent),
    Window(WindowEvent),
    FrameRead(FrameRead),
    FocusRead(FocusRead),
    Quit(Quit),
    /// A timer fired. Which timer is which node still holds that guard.
    Timer(TimerFired),
}

pub struct MercuryStruct;
impl Bindings for MercuryStruct {
    type Trigger = MercuryTrigger;
    type Event = MercuryEvent;
    type Output = Vec<MercuryEffect>;
}
