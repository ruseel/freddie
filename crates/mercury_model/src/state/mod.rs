//! State tree. Glob-imports handlers because the derive emits calls at each node's definition site.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use bind::{Bind, and, if_not_invalidated};
use freddie::{TimerGuard, timer_effect_and_guard};
use freddie_keys::{Key, KeyEvent, ModifierFlags, PressType};
use freddie_sync::{GenerationMinter, HeldGeneration, RidingGeneration, Synced};
use freddie_windows_types::{Frame, Monitor, Pid, Placement, WindowId};
use laserbeam::PathMut;

use crate::effect::emit;
#[allow(clippy::wildcard_imports)]
use crate::handlers::*;
use crate::{
    AnyKey, App, FocusLanded, ForegroundEvent, Foregrounded, FrameLanded, MercuryEffect,
    MercuryEvent, MercuryStruct, Quit, Site, TabEvent, Tabbed, Windowed,
};

mod app;
mod home;
mod nav;
mod resize;
mod return_home;
mod site;
mod typing;

pub use app::{AppData, AppLayer, ChromeApp, GhosttyApp};
pub use home::HomeLayer;
pub use nav::NavLayer;
pub use resize::ResizeLayer;
pub use return_home::{AndReturnHome, ReturnHomeLayers};
pub use site::{ClaudeAiSite, SiteData, SiteLayer};
pub(crate) use typing::arm_jk_timeout;
pub use typing::{JK_TIMEOUT, TypingLayer};

/// How long a chooser layer sits idle before returning home.
pub const RETURN_TO_HOME_TIMEOUT: Duration = Duration::from_secs(10);

/// Arm the return-home timer. Dropping the guard cancels it; the layer binds the firing on the guard it still holds.
pub(crate) fn arm_return_home() -> (TimerGuard, MercuryEffect) {
    let (guard, effect) = timer_effect_and_guard(RETURN_TO_HOME_TIMEOUT, ());
    (guard, MercuryEffect::Timer(effect))
}

/// How long the overlay stays up before its hide timer fires.
pub const OVERLAY_DWELL: Duration = Duration::from_secs(10);

#[derive(Bind, Debug)]
#[node(root)]
#[binds(MercuryStruct)]
#[bind(
    Foregrounded => if_not_invalidated(record_front_app),
    Tabbed => if_not_invalidated(record_tab_url),
    Windowed => if_not_invalidated(record_windows),
    FrameLanded => if_not_invalidated(record_frame_read),
    FocusLanded => if_not_invalidated(record_focus_read),
    Quit => if_not_invalidated(quit),
    |mercury_path| mercury_path.overlay_timer().map(TimerGuard::trigger) => if_not_invalidated(hide_overlay),
    |mercury_path| mercury_path.windows.pending_timer().map(TimerGuard::trigger) => if_not_invalidated(placement_settled),
)]
// `o` and escape bind once here. Typing's catch-all claims both first, so there they reach the app.
#[bind(
    Key::KeyO.down() => if_not_invalidated(toggle_overlay),
    Key::Escape.down() => if_not_invalidated(go_home),
    Key::F1.down() => if_not_invalidated(and!(foreground_app(App::Obsidian), enter_typing)),
    Key::F2.down() => if_not_invalidated(and!(foreground_app(App::Ghostty), enter_typing)),
    Key::F3.down() => if_not_invalidated(and!(foreground_app(App::Chrome), enter_typing)),
)]
#[post(AnyKey => track_held_modifiers)]
pub struct Mercury {
    /// Confirmed front app. `None` while a nav is in flight, so the app being left does not bind in the gap.
    pub foreground: Option<FrontApp>,
    pub generations: GenerationMinter,
    pub windows: Windows,
    /// Physical modifier keys down. Entering and leaving typing uses this to sync the app's view.
    pub held: HeldModifiers,
    /// Guard for the overlay's pending hide, or `None` if it is down. One overlay, so this lives at the root.
    overlay: Option<TimerGuard>,
    /// Active layer. Written only through [`set_layer`](Mercury::set_layer), which flushes modifiers.
    #[child]
    layer: Layer,
}

/// Chrome-only state. Nested under [`ForegroundedApp::Chrome`] so a tab URL cannot exist while Finder is up.
#[derive(Debug, Default)]
pub struct ForegroundedChrome {
    /// Front tab URL. `None` until the tab source reports, including right after Chrome comes up; a site level is unbound in that gap.
    pub url: Option<String>,
}

/// Frontmost app, with per-app state hung off the variants that have any.
#[derive(Debug, Default)]
pub enum ForegroundedApp {
    Chrome(ForegroundedChrome),
    Discord,
    Finder,
    Ghostty,
    Obsidian,
    Zed,
    #[default]
    Other,
}

impl ForegroundedApp {
    #[must_use]
    pub const fn identity(&self) -> App {
        match self {
            Self::Chrome(_) => App::Chrome,
            Self::Discord => App::Discord,
            Self::Finder => App::Finder,
            Self::Ghostty => App::Ghostty,
            Self::Obsidian => App::Obsidian,
            Self::Zed => App::Zed,
            Self::Other => App::Other,
        }
    }

    #[must_use]
    pub const fn chrome(&self) -> Option<&ForegroundedChrome> {
        match self {
            Self::Chrome(chrome) => Some(chrome),
            _ => None,
        }
    }

    #[must_use]
    pub const fn from_identity(app: App) -> Self {
        match app {
            App::Chrome => Self::Chrome(ForegroundedChrome { url: None }),
            App::Discord => Self::Discord,
            App::Finder => Self::Finder,
            App::Ghostty => Self::Ghostty,
            App::Obsidian => Self::Obsidian,
            App::Zed => Self::Zed,
            App::Other => Self::Other,
        }
    }
}

#[derive(Debug)]
pub struct FrontApp {
    pub pid: Pid,
    pub app: ForegroundedApp,
}

impl FrontApp {
    #[must_use]
    pub fn new(app: App, pid: Pid) -> Self {
        Self {
            pid,
            app: ForegroundedApp::from_identity(app),
        }
    }
}

/// Windows on screen, filled by the window source.
#[derive(Default)]
pub struct Windows {
    open: HashMap<WindowId, WindowState>,
    /// Each app's focused window, keyed by pid. `Known(None)` is a real answer: the app has no focused window with a readable id. Entries leave with `AppGone`.
    focused: HashMap<Pid, Synced<Option<WindowId>>>,
    screens: Vec<Monitor>,
    pending: Option<PendingPlacement>,
}

/// Prints `Windows` only. A full dump of every frame would bury the rest of a dispatch record.
impl fmt::Debug for Windows {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Windows")
    }
}

#[derive(Debug)]
struct WindowState {
    frame: Synced<Frame>,
    /// Frame before mercury first moved it. `None` once it is back there, or once the user has moved it since.
    restore: Option<Frame>,
}

/// A [`MercuryEffect::SetFrame`] not yet settled. Moves of this window are mercury's until the timer fires.
#[derive(Debug)]
struct PendingPlacement {
    window: WindowId,
    /// Wait ends when this fires. Drop cancels it.
    timer: TimerGuard,
}

/// How long a placement has to land. Must cover two position-and-size writes and their reports.
pub const PLACEMENT_SETTLE: Duration = Duration::from_millis(250);

impl Windows {
    /// Focused window and its frame. `None` while either sync is pending, if there is no focused window, or if focus names a window that was never opened.
    #[must_use]
    pub fn focused(&self, front: Pid) -> Option<(WindowId, Frame)> {
        let window = (*self.focused.get(&front)?.known()?)?;
        let frame = *self.open.get(&window)?.frame.known()?;
        Some((window, frame))
    }

    /// Monitor a frame's top-left is on, or the first monitor if it is on none. `None` only before the first `Screens` report.
    #[must_use]
    pub fn monitor_for(&self, frame: Frame) -> Option<Monitor> {
        self.screens
            .iter()
            .find(|m| m.full.contains(frame.x, frame.y))
            .or_else(|| self.screens.first())
            .copied()
    }

    pub(crate) fn opened(&mut self, window: WindowId, held: HeldGeneration) {
        self.open.insert(
            window,
            WindowState {
                frame: Synced::Pending(held),
                restore: None,
            },
        );
    }

    /// Whether the window is tracked (whether the caller should request the read).
    pub(crate) fn frame_change(&mut self, window: WindowId, held: HeldGeneration) -> bool {
        let ours = self.pending_covers(window);
        let Some(state) = self.open.get_mut(&window) else {
            return false;
        };
        state.frame.change(held);
        if !ours {
            state.restore = None;
        }
        true
    }

    pub(crate) fn frame_read(
        &mut self,
        window: WindowId,
        riding: &RidingGeneration,
        frame: Option<Frame>,
    ) {
        if let (Some(state), Some(frame)) = (self.open.get_mut(&window), frame) {
            state.frame.commit(riding, frame);
        }
    }

    pub(crate) fn focus_change(&mut self, pid: Pid, held: HeldGeneration) {
        self.focused.insert(pid, Synced::Pending(held));
    }

    pub(crate) fn focus_read(
        &mut self,
        pid: Pid,
        riding: &RidingGeneration,
        window: Option<WindowId>,
    ) {
        if let Some(entry) = self.focused.get_mut(&pid) {
            entry.commit(riding, window);
        }
    }

    pub(crate) fn closed(&mut self, window: WindowId) {
        self.open.remove(&window);
    }

    pub(crate) fn app_gone(&mut self, pid: Pid) {
        self.focused.remove(&pid);
    }

    /// Screens changed. The watcher reads them synchronously, so the value rides the event.
    pub(crate) fn screens_changed(&mut self, screens: &[Monitor]) {
        self.screens = screens.to_vec();
    }

    /// Whether `moved` is mercury's outstanding placement. The wait ends on the timer, not on the asked-for frame: one placement writes position and size twice, and ending on the first report would treat the rest as a user drag.
    fn pending_covers(&self, moved: WindowId) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.window == moved)
    }

    pub(crate) fn pending_timer(&self) -> Option<&TimerGuard> {
        self.pending.as_ref().map(|p| &p.timer)
    }

    pub(crate) fn forget_pending(&mut self) {
        self.pending = None;
    }

    /// Ask for the placement and start the settle wait. Leaves the remembered frame alone.
    fn asking_for(&mut self, placement: Placement) -> Vec<MercuryEffect> {
        let (timer, effect) = timer_effect_and_guard(PLACEMENT_SETTLE, ());
        self.pending = Some(PendingPlacement {
            window: placement.window,
            timer,
        });
        vec![
            MercuryEffect::SetFrame(placement),
            MercuryEffect::Timer(effect),
        ]
    }

    /// Remember the current frame if none is remembered, then place. A run of placements restore to where the window was before the first.
    pub(crate) fn placing(&mut self, placement: Placement) -> Vec<MercuryEffect> {
        let Some(state) = self.open.get_mut(&placement.window) else {
            return Vec::new();
        };
        state.restore.get_or_insert(placement.from);
        self.asking_for(placement)
    }

    /// Put the focused window back. Takes the remembered frame, so a second restore has nothing to do.
    pub(crate) fn restoring(&mut self, front: Pid) -> Vec<MercuryEffect> {
        let Some((window, from)) = self.focused(front) else {
            return Vec::new();
        };
        let Some(to) = self
            .open
            .get_mut(&window)
            .and_then(|state| state.restore.take())
        else {
            return Vec::new();
        };
        self.asking_for(Placement { window, from, to })
    }
}

#[derive(Bind, Debug, derive_more::From)]
#[node(parent_path = MercuryPath)]
#[binds(MercuryStruct)]
pub enum Layer {
    Home(HomeLayer),
    Typing(TypingLayer),
    ReturnHome(AndReturnHome<ReturnHomeLayers>),
}

impl Layer {
    /// Whether keys pass through to the app. Entering and leaving one flushes modifiers. Typing is the only such layer.
    #[must_use]
    pub const fn is_passthrough(&self) -> bool {
        matches!(self, Self::Typing(_))
    }

    /// Overlay keymap. In-app and site arms fall back to the generic card while a nav is in flight.
    #[must_use]
    pub fn overlay_content(&self, foreground: Option<&ForegroundedApp>) -> &'static str {
        match self {
            Self::Home(_) => home::OVERLAY,
            Self::Typing(_) => typing::OVERLAY,
            Self::ReturnHome(w) => match w.layers() {
                ReturnHomeLayers::Nav(_) => nav::OVERLAY,
                ReturnHomeLayers::Resize(_) => resize::OVERLAY,
                ReturnHomeLayers::InApp(_) => {
                    app::overlay_for(foreground.map_or(App::Other, ForegroundedApp::identity))
                }
                // Site keymap is the front tab's, so it needs the URL.
                ReturnHomeLayers::Site(_) => site::overlay_for(
                    foreground
                        .and_then(ForegroundedApp::chrome)
                        .and_then(|chrome| chrome.url.as_deref())
                        .map(Site::from_url),
                ),
            },
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Home(_) => "Home",
            Self::Typing(_) => "Typing",
            Self::ReturnHome(w) => match w.layers() {
                ReturnHomeLayers::Nav(_) => "Nav",
                ReturnHomeLayers::Resize(_) => "Resize",
                ReturnHomeLayers::InApp(_) => "App",
                ReturnHomeLayers::Site(_) => "Site",
            },
        }
    }
}

/// `&mut Mercury`; children name this as their parent path.
pub type MercuryPath<'a> = &'a mut Mercury;
pub type LayerPath<'a> = PathMut<Layer, MercuryPath<'a>>;
pub type TypingLayerPath<'a> = PathMut<TypingLayer, LayerPath<'a>>;
pub type AndReturnHomePath<'a> = PathMut<AndReturnHome<ReturnHomeLayers>, LayerPath<'a>>;
pub type ReturnHomeLayersPath<'a> = PathMut<ReturnHomeLayers, AndReturnHomePath<'a>>;
pub type AppLayerPath<'a> = PathMut<AppLayer, ReturnHomeLayersPath<'a>>;
pub type SiteLayerPath<'a> = PathMut<SiteLayer, ReturnHomeLayersPath<'a>>;

impl Mercury {
    /// Boot layer: Typing, so a mercury launched at login does not swallow the keyboard.
    fn boot_layer() -> Layer {
        Layer::Typing(TypingLayer::new())
    }

    /// Status item title before the first layer change. A literal because `boot_layer().name()` is not const; `boot_title_matches_the_boot_layer` guards drift.
    pub const BOOT_TITLE: &'static str = "Typing";

    /// Model at boot. No `Default`: an untold front app would resolve the in-app layer against the wrong app.
    #[must_use]
    pub fn new(front: Option<FrontApp>, windows: Windows) -> Self {
        Self {
            foreground: front,
            generations: GenerationMinter::default(),
            windows,
            held: HeldModifiers::default(),
            overlay: None,
            layer: Self::boot_layer(),
        }
    }

    /// For tests. A live transition goes through [`set_layer`](Self::set_layer).
    #[must_use]
    pub fn with_layer(layer: Layer) -> Self {
        Self {
            layer,
            ..Self::new(None, Windows::default())
        }
    }

    #[must_use]
    pub fn handle(&mut self, event: &MercuryEvent) -> Vec<MercuryEffect> {
        bind::dispatch::<MercuryStruct, Self, _>(self, event)
    }

    #[must_use]
    pub const fn layer(&self) -> &Layer {
        &self.layer
    }

    #[must_use = "the returned effects put the overlay up or take it down"]
    pub fn toggle_overlay(&mut self) -> Vec<MercuryEffect> {
        if self.overlay.is_some() {
            return self.hide_overlay();
        }
        let content = self
            .layer
            .overlay_content(self.foreground.as_ref().map(|front| &front.app));
        let (guard, effect) = timer_effect_and_guard(OVERLAY_DWELL, ());
        self.overlay = Some(guard);
        vec![
            MercuryEffect::ShowOverlay(content),
            MercuryEffect::Timer(effect),
        ]
    }

    /// Take the overlay down. Taking the field drops the guard, cancelling a hide that has not fired.
    #[must_use = "the returned effect takes the overlay off the screen"]
    pub fn hide_overlay(&mut self) -> Vec<MercuryEffect> {
        if self.overlay.take().is_some() {
            vec![MercuryEffect::HideOverlay]
        } else {
            Vec::new()
        }
    }

    #[must_use]
    pub const fn overlay_timer(&self) -> Option<&TimerGuard> {
        self.overlay.as_ref()
    }

    /// Replace the active layer. Flushes modifiers only when entering or leaving a passthrough layer: `close` on leaving typing, `open` on entering it.
    #[must_use = "the returned flush has to be emitted, or a held modifier is stranded down"]
    pub fn set_layer(&mut self, into: impl Into<Layer>) -> Vec<MercuryEffect> {
        let into = into.into();
        let before_passthrough = self.layer.is_passthrough();
        let after_passthrough = into.is_passthrough();
        self.layer = into;
        let mut effects = self.hide_overlay();
        effects.extend(match (before_passthrough, after_passthrough) {
            (true, false) => self.held.close(),
            (false, true) => self.held.open(),
            _ => Vec::new(),
        });
        effects.push(MercuryEffect::ShowLayer(self.layer.name()));
        effects
    }
}

/// Left and right keys of one modifier. The flag is set while either is down.
#[derive(Debug, Default, Clone, Copy)]
pub struct LeftRightPair {
    pub left: bool,
    pub right: bool,
}

#[derive(Clone, Copy)]
pub enum Side {
    Left,
    Right,
}

impl LeftRightPair {
    #[must_use]
    pub const fn any_held(self) -> bool {
        self.left || self.right
    }

    pub const fn set(&mut self, side: Side, is_down: bool) {
        match side {
            Side::Left => self.left = is_down,
            Side::Right => self.right = is_down,
        }
    }
}

/// Physical modifier keys down. Caps lock is a lock, not a held key, so it is not here.
#[derive(Default, Clone, Copy)]
pub struct HeldModifiers {
    pub control: LeftRightPair,
    pub meta: LeftRightPair,
    pub alt: LeftRightPair,
    pub shift: LeftRightPair,
}

impl std::fmt::Debug for HeldModifiers {
    /// `HeldModifiers { Meta(L,R), Alt(L) }`, or `HeldModifiers {}` if none.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HeldModifiers {{")?;
        let mut any = false;
        for (name, pair) in [
            ("Control", self.control),
            ("Meta", self.meta),
            ("Alt", self.alt),
            ("Shift", self.shift),
        ] {
            let sides = match (pair.left, pair.right) {
                (true, true) => "(L,R)",
                (true, false) => "(L)",
                (false, true) => "(R)",
                (false, false) => continue,
            };
            write!(f, "{}{name}{sides}", if any { ", " } else { " " })?;
            any = true;
        }
        f.write_str(if any { " }" } else { "}" })
    }
}

impl HeldModifiers {
    pub fn apply(&mut self, ev: &KeyEvent) {
        let is_down = ev.press == PressType::Down;
        match ev.key {
            Key::ControlLeft => self.control.set(Side::Left, is_down),
            Key::ControlRight => self.control.set(Side::Right, is_down),
            Key::MetaLeft => self.meta.set(Side::Left, is_down),
            Key::MetaRight => self.meta.set(Side::Right, is_down),
            Key::AltLeft => self.alt.set(Side::Left, is_down),
            Key::AltRight => self.alt.set(Side::Right, is_down),
            Key::ShiftLeft => self.shift.set(Side::Left, is_down),
            Key::ShiftRight => self.shift.set(Side::Right, is_down),
            _ => {}
        }
    }

    /// Downs for every held key, so the app catches up on entering a passthrough layer.
    #[must_use]
    pub fn open(self) -> Vec<MercuryEffect> {
        self.emit_synchronization_events(PressType::Down)
    }

    /// Ups for every held key, so the app forgets them on leaving a passthrough layer.
    #[must_use]
    pub fn close(self) -> Vec<MercuryEffect> {
        self.emit_synchronization_events(PressType::Up)
    }

    /// Emit `press` for every held key, each carrying flags as they stand after that key's own change, so a shared left/right bit clears only when both sides are up.
    fn emit_synchronization_events(self, press: PressType) -> Vec<MercuryEffect> {
        let mut shown = if press == PressType::Down {
            Self::default()
        } else {
            self
        };
        let mut out = Vec::new();
        for key in self.held_keys() {
            shown.apply(&KeyEvent {
                key,
                press,
                flags: ModifierFlags::empty(),
            });
            out.push(emit(key, press, shown.flags()));
        }
        out
    }

    fn held_keys(&self) -> impl Iterator<Item = Key> {
        [
            (Key::ControlLeft, self.control.left),
            (Key::ControlRight, self.control.right),
            (Key::MetaLeft, self.meta.left),
            (Key::MetaRight, self.meta.right),
            (Key::AltLeft, self.alt.left),
            (Key::AltRight, self.alt.right),
            (Key::ShiftLeft, self.shift.left),
            (Key::ShiftRight, self.shift.right),
        ]
        .into_iter()
        .filter_map(|(key, held)| held.then_some(key))
    }

    #[must_use]
    pub const fn flags(self) -> ModifierFlags {
        let mut f = ModifierFlags::empty();
        f.set(ModifierFlags::CONTROL, self.control.any_held());
        f.set(ModifierFlags::COMMAND, self.meta.any_held());
        f.set(ModifierFlags::ALT, self.alt.any_held());
        f.set(ModifierFlags::SHIFT, self.shift.any_held());
        f
    }
}

#[must_use]
pub const fn key(key: Key) -> MercuryEvent {
    MercuryEvent::Key(KeyEvent {
        key,
        press: PressType::Down,
        flags: ModifierFlags::empty(),
    })
}

#[must_use]
pub const fn foreground(app: App, pid: Pid) -> MercuryEvent {
    MercuryEvent::Foreground(ForegroundEvent { app, pid })
}

#[must_use]
pub const fn frame_read(
    window: WindowId,
    generation: RidingGeneration,
    frame: Option<Frame>,
) -> MercuryEvent {
    MercuryEvent::FrameRead(crate::FrameRead {
        window,
        generation,
        frame,
    })
}

#[must_use]
pub const fn focus_read(
    pid: Pid,
    generation: RidingGeneration,
    window: Option<WindowId>,
) -> MercuryEvent {
    MercuryEvent::FocusRead(crate::FocusRead {
        pid,
        generation,
        window,
    })
}

#[must_use]
pub const fn tab(url: String) -> MercuryEvent {
    MercuryEvent::Tab(TabEvent { url })
}

#[must_use]
pub const fn quit_event() -> MercuryEvent {
    MercuryEvent::Quit(Quit)
}
