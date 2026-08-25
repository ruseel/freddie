//! Shared laserbeam + bind tree for the accumulate and dispatch tests. Handlers return the fired key's length so a dispatch test can see which one ran.
#![expect(dead_code)]

use bind::{AscendState, Bind, Bindings, EventTrigger};
use laserbeam::{Above, Completed, CompletesTo, HasStop, MaybeInvalidated, PathMut};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Keyboard(pub &'static str);
pub struct KeyEvent {
    pub key: &'static str,
}
impl EventTrigger for Keyboard {
    type Event = KeyEvent;
    fn is_matching(&self, ev: &KeyEvent) -> bool {
        self.0 == ev.key
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Foreground(pub &'static str);
pub struct FgEvent {
    pub app: &'static str,
}
impl EventTrigger for Foreground {
    type Event = FgEvent;
    fn is_matching(&self, ev: &FgEvent) -> bool {
        self.0 == ev.app
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum DemoTrigger {
    Keyboard(Keyboard),
    Foreground(Foreground),
    WaitingFor(WaitingFor),
}
impl From<Keyboard> for DemoTrigger {
    fn from(k: Keyboard) -> Self {
        Self::Keyboard(k)
    }
}
impl From<Foreground> for DemoTrigger {
    fn from(f: Foreground) -> Self {
        Self::Foreground(f)
    }
}

pub enum DemoEvent {
    Keyboard(KeyEvent),
    Foreground(FgEvent),
}
impl<'a> TryFrom<&'a DemoEvent> for &'a KeyEvent {
    type Error = ();
    fn try_from(e: &'a DemoEvent) -> Result<Self, ()> {
        match e {
            DemoEvent::Keyboard(k) => Ok(k),
            DemoEvent::Foreground(_) => Err(()),
        }
    }
}
impl<'a> TryFrom<&'a DemoEvent> for &'a FgEvent {
    type Error = ();
    fn try_from(e: &'a DemoEvent) -> Result<Self, ()> {
        match e {
            DemoEvent::Foreground(f) => Ok(f),
            DemoEvent::Keyboard(_) => Err(()),
        }
    }
}

pub struct Demo;
impl Bindings for Demo {
    type Trigger = DemoTrigger;
    type Event = DemoEvent;
    type Output = Vec<usize>;
}

pub fn on_esc<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, AppPath<'x>>,
) -> (Vec<usize>, Completed<AppPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(app) => {
            app.hits += 1;
            (vec![ev.key.len()], app.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}
pub fn on_f1<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, LayerPath<'x>>,
) -> (Vec<usize>, Completed<LayerPath<'x>>) {
    (vec![ev.key.len()], st.complete())
}
pub fn on_g<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, NavPath<'x>>,
) -> (Vec<usize>, Completed<NavPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut nav) => {
            nav.get_mut().hits += 1;
            (vec![ev.key.len()], nav.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}
pub fn on_slack<'x>(
    ev: &FgEvent,
    _snap: (),
    st: AscendState<'_, NavPath<'x>>,
) -> (Vec<usize>, Completed<NavPath<'x>>) {
    (vec![ev.app.len()], st.complete())
}
pub fn on_bksp<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, TypingPath<'x>>,
) -> (Vec<usize>, Completed<TypingPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut typing) => {
            typing.get_mut().hits += 1;
            (vec![ev.key.len()], typing.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}
pub fn on_d<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, DeepPath<'x>>,
) -> (Vec<usize>, Completed<DeepPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut deep) => {
            deep.get_mut().hits += 1;
            (vec![ev.key.len()], deep.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}
pub fn on_armed<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ArmedPath<'x>>,
) -> (Vec<usize>, Completed<ArmedPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(armed) => {
            armed.waiting_for = None;
            (vec![ev.key.len()], armed.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}

/// Binds at any place: reads nothing, names no node.
pub fn ignore<P: HasStop + CompletesTo<P>>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, P>,
) -> (Vec<usize>, Completed<P>) {
    (vec![ev.key.len()], st.complete())
}

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
#[bind(Keyboard("esc") => on_esc)]
pub struct App {
    pub hits: u32,
    #[child]
    pub layer: Layer,
}

#[derive(Bind)]
#[node(parent_path = AppPath)]
#[binds(Demo)]
#[bind(Keyboard("f1") => on_f1)]
pub enum Layer {
    Nav(Nav),
    Typing(Typing),
}

#[derive(Bind)]
#[node(parent_path = LayerPath)]
#[binds(Demo)]
#[bind(Keyboard("g") => on_g, Foreground("Slack") => on_slack)]
pub struct Nav {
    pub hits: u32,
}

#[derive(Bind)]
#[node(parent_path = LayerPath)]
#[binds(Demo)]
#[bind(Keyboard("bksp") => on_bksp)]
pub struct Typing {
    pub hits: u32,
    #[child]
    pub deep: Box<Deep>,
}

#[derive(Bind)]
#[node(parent_path = TypingPath)]
#[binds(Demo)]
#[bind(Keyboard("d") => on_d)]
pub struct Deep {
    pub hits: u32,
}

pub type AppPath<'a> = &'a mut App;
pub type LayerPath<'a> = PathMut<Layer, AppPath<'a>>;
pub type NavPath<'a> = PathMut<Nav, LayerPath<'a>>;
pub type TypingPath<'a> = PathMut<Typing, LayerPath<'a>>;
pub type DeepPath<'a> = PathMut<Deep, TypingPath<'a>>;

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
#[bind(Keyboard("dup") => ignore)]
pub struct Clash {
    #[child]
    pub child: ClashChild,
}

#[derive(Bind)]
#[node(parent_path = ClashPath)]
#[binds(Demo)]
#[bind(Keyboard("dup") => ignore)]
pub struct ClashChild;

pub type ClashPath<'a> = &'a mut Clash;
pub type ClashChildPath<'a> = PathMut<ClashChild, ClashPath<'a>>;

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
pub struct Empty;

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
pub enum Media {
    Album(Album),
    Song(Song),
}

#[derive(Bind)]
#[node(parent_path = MediaPath)]
#[binds(Demo)]
#[bind(Keyboard("a") => ignore)]
pub struct Album {
    #[child(route = TitleParent, up = TitleParentUp)]
    pub title: Title,
}

#[derive(Bind)]
#[node(parent_path = MediaPath)]
#[binds(Demo)]
#[bind(Keyboard("s") => ignore)]
pub struct Song {
    #[child(route = TitleParent, up = TitleParentUp)]
    pub title: Title,
}

#[derive(Bind)]
#[node(parent_path = TitleParent)]
#[binds(Demo)]
#[bind(Keyboard("t") => on_title, Keyboard("home") => title_home)]
pub struct Title {
    pub hits: u32,
}

pub type MediaPath<'a> = &'a mut Media;
pub type AlbumPath<'a> = PathMut<Album, MediaPath<'a>>;
pub type SongPath<'a> = PathMut<Song, MediaPath<'a>>;
pub enum TitleParent<'a> {
    Album(AlbumPath<'a>),
    Song(SongPath<'a>),
}

/// `Above::Up` for `TitleParent`. The consumer writes this; laserbeam cannot, because a route enum is not a `PathMut`.
pub enum TitleParentUp<'a> {
    Album(Completed<AlbumPath<'a>>),
    Song(Completed<SongPath<'a>>),
}

impl<'a> Above for TitleParent<'a> {
    type Up = TitleParentUp<'a>;
}

pub type TitlePath<'a> = PathMut<Title, TitleParent<'a>>;
pub fn on_title<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, TitlePath<'x>>,
) -> (Vec<usize>, Completed<TitlePath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut title) => {
            title.get_mut().hits += 1;
            (vec![ev.key.len()], title.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}

/// Leave from `Title` to the root. `into_parent()` yields the route enum, which has no `into_parent`, so the leave matches it and wraps one `Up` by hand.
pub fn title_home<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, TitlePath<'x>>,
) -> (Vec<usize>, Completed<TitlePath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(title) => {
            let up = match title.into_parent() {
                TitleParent::Album(album) => TitleParentUp::Album(album.into_parent().complete()),
                TitleParent::Song(song) => TitleParentUp::Song(song.into_parent().complete()),
            };
            (vec![], Completed::up(up))
        }
        MaybeInvalidated::Invalidated(c) => (vec![], c),
    }
}

pub const fn kb(s: &'static str) -> DemoTrigger {
    DemoTrigger::Keyboard(Keyboard(s))
}
pub const fn fg(s: &'static str) -> DemoTrigger {
    DemoTrigger::Foreground(Foreground(s))
}
pub const fn key(s: &'static str) -> DemoEvent {
    DemoEvent::Keyboard(KeyEvent { key: s })
}
#[must_use]
pub const fn waiting(k: Option<&'static str>) -> DemoTrigger {
    DemoTrigger::WaitingFor(WaitingFor(k))
}

pub const fn foreground(s: &'static str) -> DemoEvent {
    DemoEvent::Foreground(FgEvent { app: s })
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct WaitingFor(pub Option<&'static str>);

impl EventTrigger for WaitingFor {
    type Event = KeyEvent;
    fn is_matching(&self, ev: &KeyEvent) -> bool {
        self.0 == Some(ev.key)
    }
}

impl From<WaitingFor> for DemoTrigger {
    fn from(w: WaitingFor) -> Self {
        Self::WaitingFor(w)
    }
}

pub type ArmedPath<'a> = &'a mut Armed;
pub type ArmedChildPath<'a> = PathMut<ArmedChild, ArmedPath<'a>>;

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
#[bind(
    |armed_path| WaitingFor(armed_path.waiting_for) => on_armed,
    Keyboard("esc") => on_esc_armed,
)]
pub struct Armed {
    pub waiting_for: Option<&'static str>,
    /// Separate from `waiting_for` so the child's parent-reading bind cannot collide with this node's own trigger.
    pub for_child: Option<&'static str>,
    #[child]
    pub child: ArmedChild,
}

#[derive(Bind)]
#[node(parent_path = ArmedPath)]
#[binds(Demo)]
#[bind(
    |armed_child_path| armed_child_path.get().wants.map(Keyboard) => on_child_armed,
    |armed_child_path| Keyboard(armed_child_path.parent().for_child.unwrap_or("none")) => on_parents_key,
)]
pub struct ArmedChild {
    pub wants: Option<&'static str>,
}

pub fn on_parents_key<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ArmedChildPath<'x>>,
) -> (Vec<usize>, Completed<ArmedChildPath<'x>>) {
    (vec![ev.key.len() + 100], st.complete())
}

pub fn on_esc_armed<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ArmedPath<'x>>,
) -> (Vec<usize>, Completed<ArmedPath<'x>>) {
    (vec![ev.key.len()], st.complete())
}

pub fn on_child_armed<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ArmedChildPath<'x>>,
) -> (Vec<usize>, Completed<ArmedChildPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut child) => {
            child.get_mut().wants = None;
            (vec![ev.key.len()], child.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}
