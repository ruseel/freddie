//! A level that is not in the tree. Two derived levels, one under the other: a derived level can have a derived child, and a miss hands the parent back at every level.

mod common;

use std::fmt::Write as _;

use bind::{AscendState, Bind, DerivedLevel, accumulate, dispatch, exclusive};
use common::{Demo, DemoEvent, KeyEvent, Keyboard, kb};
use laserbeam::{Completed, CompletesTo, MaybeInvalidated, PathMut};
use std::collections::HashSet;

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
pub struct Root {
    pub app: Option<Chrome>,
    #[child]
    pub layer: Shell,
}

pub struct Chrome {
    pub tab: String,
}

#[derive(Bind)]
#[node(parent_path = RootPath)]
#[binds(Demo)]
#[derived_children(app_data)]
#[post(Keyboard("q") => log_leave)]
#[bind(Keyboard("esc") => on_esc)]
pub struct Shell {
    pub log: String,
}

#[derive(Bind)]
#[derived_node(parent_path = ShellPath)]
#[binds(Demo)]
#[derived_children(tab_data)]
#[pre_post(Keyboard("r") => (snap_tab, exclusive(on_r)))]
#[bind(Keyboard("q") => app_home)]
pub struct AppData {
    pub tab: String,
}

#[derive(Bind)]
#[derived_node(parent_path = AppNode)]
#[binds(Demo)]
#[pre_post(Keyboard("g") => (snap_tab_thread, exclusive(on_g)))]
pub struct TabData {
    pub thread: u32,
}

pub type RootPath<'a> = &'a mut Root;
pub type ShellPath<'a> = PathMut<Shell, RootPath<'a>>;
pub type AppNode<'a> = DerivedLevel<ShellPath<'a>, AppData>;
pub type TabNode<'a> = DerivedLevel<AppNode<'a>, TabData>;

pub enum R<'a> {
    Shell(ShellPath<'a>),
}

fn app_data(path: &ShellPath) -> Option<AppData> {
    let chrome = path.parent().app.as_ref()?;
    Some(AppData {
        tab: chrome.tab.clone(),
    })
}

fn tab_data(node: &AppNode) -> Option<TabData> {
    (node.data.tab == "gmail").then_some(TabData { thread: 7 })
}

/// Takes the level's data while the node is whole: descent consumes it, ascent holds only the place beneath.
fn snap_tab(_ev: &KeyEvent, node: &AppNode) -> String {
    node.data.tab.clone()
}

/// Writes the snap into the layer. The snap is owned because the node it was read from is consumed by descent before ascent runs.
#[expect(clippy::needless_pass_by_value)]
fn on_r<'x>(
    ev: &KeyEvent,
    tab: String,
    st: AscendState<'_, ShellPath<'x>>,
) -> (Vec<usize>, Completed<ShellPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut shell) => {
            shell.get_mut().log.push_str(&tab);
            (vec![ev.key.len()], shell.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}

fn snap_tab_thread(_ev: &KeyEvent, node: &TabNode) -> (String, u32) {
    (node.parent.data.tab.clone(), node.data.thread)
}

fn on_g<'x>(
    ev: &KeyEvent,
    (tab, thread): (String, u32),
    st: AscendState<'_, ShellPath<'x>>,
) -> (Vec<usize>, Completed<ShellPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut shell) => {
            let _ = write!(shell.get_mut().log, "{tab}{thread}");
            (vec![ev.key.len()], shell.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}

fn on_esc<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ShellPath<'x>>,
) -> (Vec<usize>, Completed<ShellPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut shell) => {
            shell.get_mut().log.push('e');
            (vec![3], shell.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![3], c),
    }
}

fn app_home<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ShellPath<'x>>,
) -> (Vec<usize>, Completed<ShellPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(shell) => (vec![9], shell.into_parent().complete()),
        MaybeInvalidated::Invalidated(c) => (vec![9], c),
    }
}

fn log_leave<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ShellPath<'x>>,
) -> (Vec<usize>, Completed<ShellPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut shell) => {
            shell.get_mut().log.push('s');
            (vec![], shell.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![7], c),
    }
}

const fn key(k: &'static str) -> DemoEvent {
    DemoEvent::Keyboard(KeyEvent { key: k })
}

fn root(tab: Option<&str>) -> Root {
    Root {
        app: tab.map(|t| Chrome { tab: t.to_owned() }),
        layer: Shell { log: String::new() },
    }
}

#[test]
fn a_derived_level_binds_its_own_keys_and_reaches_the_tree_through_parent() {
    let mut r = root(Some("inbox"));
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("r")), vec![1]);
    assert_eq!(r.layer.log, "inbox");
    assert_eq!(r.app.as_ref().unwrap().tab, "inbox");
}

#[test]
fn a_derived_level_can_have_a_derived_child() {
    let mut r = root(Some("gmail"));
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("g")), vec![1]);
    assert_eq!(r.layer.log, "gmail7");
}

#[test]
fn a_miss_hands_the_parent_back_at_every_level() {
    let mut r = root(Some("gmail"));
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("r")), vec![1]);
    assert_eq!(r.layer.log, "gmail");

    let mut r = root(Some("gmail"));
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("esc")), vec![3]);
    assert_eq!(r.layer.log, "e");
}

#[test]
fn with_no_app_there_is_no_level_and_the_layer_still_works() {
    let mut r = root(None);
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("r")), vec![]);
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("esc")), vec![3]);
    assert_eq!(r.layer.log, "e");
}

#[test]
fn the_check_sees_a_derived_levels_binds() {
    // `r` and `g` are `#[pre_post]`; the check collects `#[bind]` only. Shell's `q` post is absent for the same reason, so it can share a trigger with the app level's bind.
    let mut r = root(Some("gmail"));
    let set: HashSet<_> = accumulate::<Demo, Root>(&mut r).unwrap();
    assert_eq!(set, HashSet::from([kb("esc"), kb("q")]));

    let mut r = root(Some("inbox"));
    let set: HashSet<_> = accumulate::<Demo, Root>(&mut r).unwrap();
    assert_eq!(set, HashSet::from([kb("esc"), kb("q")]));

    let mut r = root(None);
    let set: HashSet<_> = accumulate::<Demo, Root>(&mut r).unwrap();
    assert_eq!(set, HashSet::from([kb("esc")]));
}

#[test]
fn a_leave_from_a_derived_level_reaches_the_place_as_invalidated() {
    let mut r = root(Some("gmail"));
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("q")), vec![9, 7]);
    assert_eq!(r.layer.log, "", "the post's staying arm never ran");
}

#[test]
fn the_post_marks_the_layer_when_nothing_left() {
    let mut r = root(None);
    assert_eq!(dispatch::<Demo, Root, _>(&mut r, &key("q")), vec![]);
    assert_eq!(r.layer.log, "s");
}

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
pub struct Modes {
    pub mode: Option<bool>,
    #[child]
    pub shell: ModeShell,
}

#[derive(Bind)]
#[node(parent_path = ModesPath)]
#[binds(Demo)]
#[derived_children(mode_data)]
pub struct ModeShell {
    pub log: String,
}

#[derive(Bind)]
#[derived_node(parent_path = ModeShellPath)]
#[binds(Demo)]
pub enum ModeData {
    On(OnMode),
    Off(OffMode),
}

#[derive(Bind)]
#[derived_node(parent_path = ModeShellPath)]
#[binds(Demo)]
#[bind(Keyboard("m") => on_mode_on)]
pub struct OnMode;

#[derive(Bind)]
#[derived_node(parent_path = ModeShellPath)]
#[binds(Demo)]
#[bind(Keyboard("m") => on_mode_off)]
pub struct OffMode;

pub type ModesPath<'a> = &'a mut Modes;
pub type ModeShellPath<'a> = PathMut<ModeShell, ModesPath<'a>>;

fn mode_data(path: &ModeShellPath) -> Option<ModeData> {
    let on = path.parent().mode?;
    Some(if on {
        ModeData::On(OnMode)
    } else {
        ModeData::Off(OffMode)
    })
}

fn on_mode_on<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ModeShellPath<'x>>,
) -> (Vec<usize>, Completed<ModeShellPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut shell) => {
            shell.get_mut().log.push_str("on");
            (vec![ev.key.len()], shell.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}

fn on_mode_off<'x>(
    ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, ModeShellPath<'x>>,
) -> (Vec<usize>, Completed<ModeShellPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut shell) => {
            shell.get_mut().log.push_str("off");
            (vec![ev.key.len()], shell.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![ev.key.len()], c),
    }
}

const fn modes(mode: Option<bool>) -> Modes {
    Modes {
        mode,
        shell: ModeShell { log: String::new() },
    }
}

#[test]
fn the_live_variant_of_a_derived_enum_handles_the_key() {
    let mut m = modes(Some(true));
    assert_eq!(dispatch::<Demo, Modes, _>(&mut m, &key("m")), vec![1]);
    assert_eq!(m.shell.log, "on");

    let mut m = modes(Some(false));
    assert_eq!(dispatch::<Demo, Modes, _>(&mut m, &key("m")), vec![1]);
    assert_eq!(m.shell.log, "off");
}

#[test]
fn no_mode_is_no_level_at_all() {
    let mut m = modes(None);
    assert_eq!(dispatch::<Demo, Modes, _>(&mut m, &key("m")), vec![]);
    assert_eq!(m.shell.log, "");
}

#[test]
fn the_check_sees_only_the_live_variants_trigger() {
    let mut m = modes(Some(true));
    let set: HashSet<_> = accumulate::<Demo, Modes>(&mut m).unwrap();
    assert_eq!(set, HashSet::from([kb("m")]));

    let mut m = modes(None);
    let set: HashSet<_> = accumulate::<Demo, Modes>(&mut m).unwrap();
    assert_eq!(set, HashSet::new());
}
