//! mercury's model: a pure function of state and event.
//!
//! This crate cannot call macOS: no platform crate in the graph, `unsafe` forbidden, clippy denies std's OS surface. The one impurity is timer-guard minting through `freddie::timer_effect_and_guard`, which is channel construction.

pub use freddie_keys::{Key, KeyEvent, KeyPress, ModifierFlags, MouseButtonEvent, PressType};

mod effect;
mod handlers;
mod model;
mod sources;
mod state;

pub use effect::{Chord, MercuryEffect, UrlPart};
pub use freddie_windows_types::{Pid, Placement};
pub use model::{MercuryEvent, MercuryStruct, MercuryTrigger};
pub use sources::{
    AnyKey, App, FocusLanded, FocusRead, ForegroundEvent, Foregrounded, FrameLanded, FrameRead,
    MouseButtonPressed, Quit, Site, TabEvent, Tabbed, WindowEvent, Windowed, host,
};
pub use state::{
    AndReturnHome, AppData, AppLayer, ChromeApp, ClaudeAiSite, ForegroundedApp, ForegroundedChrome,
    FrontApp, GhosttyApp, HeldMouseButtons, HomeLayer, JK_TIMEOUT, Layer, MouseButtonHoldState,
    Mercury, NavLayer, OVERLAY_DWELL, PLACEMENT_SETTLE, RETURN_TO_HOME_TIMEOUT, ResizeLayer,
    ReturnHomeLayers, SiteData, SiteLayer, TypingLayer, Windows, focus_read, foreground, frame_read,
    key, quit_event, tab,
};
