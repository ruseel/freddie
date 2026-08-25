//! Schedule over an `A -> B` tree. `B` arms a return-home timer; a leave has to cancel it because the OS timer outlives the active path and `Drop` cannot emit the cancel.

use bind::{AscendState, Bind, Bindings, EventTrigger, and, dispatch, if_not_invalidated};
use laserbeam::{Completed, CompletesTo, HasStop, IntoAncestor, MaybeInvalidated, PathMut};

pub struct KeyEvent {
    pub key: &'static str,
}

/// Matches every key.
pub struct AnyKey;
impl EventTrigger for AnyKey {
    type Event = KeyEvent;
    fn is_matching(&self, _ev: &KeyEvent) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key(pub &'static str);
impl EventTrigger for Key {
    type Event = KeyEvent;
    fn is_matching(&self, ev: &KeyEvent) -> bool {
        self.0 == ev.key
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum DemoTrigger {
    Key(Key),
}
impl From<Key> for DemoTrigger {
    fn from(k: Key) -> Self {
        Self::Key(k)
    }
}

pub enum DemoEvent {
    Key(KeyEvent),
}
impl<'a> TryFrom<&'a DemoEvent> for &'a KeyEvent {
    type Error = ();
    fn try_from(e: &'a DemoEvent) -> Result<Self, ()> {
        let DemoEvent::Key(k) = e;
        Ok(k)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TimerId(pub u64);

impl TimerId {
    /// Fixed id so a walk can name it.
    const fn fresh() -> Self {
        Self(1)
    }
}

#[derive(Debug)]
pub struct TimerGuard {
    pub id: TimerId,
}

#[derive(PartialEq, Eq, Debug)]
pub enum DemoEffect {
    ScheduleTimer(TimerId),
    CancelTimer(TimerId),
    FlashOverlay,
    SawStanding,
    SawInvalidated,
}

pub struct M;
impl Bindings for M {
    type Trigger = DemoTrigger;
    type Event = DemoEvent;
    type Output = Vec<DemoEffect>;
}

const fn key(k: &'static str) -> DemoEvent {
    DemoEvent::Key(KeyEvent { key: k })
}

#[derive(Bind)]
#[node(root)]
#[binds(M)]
#[pre_post(AnyKey => (snap_return_home, return_home_deadline))]
#[bind(Key("esc") => flash)]
pub struct A {
    #[child]
    pub b: B,
}

#[derive(Bind)]
#[node(parent_path = APath)]
#[binds(M)]
#[bind(Key("h") => go_home, Key("bump") => bump_timer)]
pub struct B {
    pub return_home: TimerGuard,
}

pub type APath<'a> = &'a mut A;
pub type BPath<'a> = PathMut<B, APath<'a>>;

fn go_home<'x, P>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, P>,
) -> (Vec<DemoEffect>, Completed<P>)
where
    P: HasStop,
    MaybeInvalidated<P>: IntoAncestor<APath<'x>>,
    APath<'x>: CompletesTo<P>,
{
    (vec![], st.state.into_ancestor::<APath<'x>>().complete())
}

fn bump_timer<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, BPath<'x>>,
) -> (Vec<DemoEffect>, Completed<BPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(mut b) => {
            b.get_mut().return_home = TimerGuard { id: TimerId(99) };
            (vec![], b.complete())
        }
        MaybeInvalidated::Invalidated(c) => (vec![], c),
    }
}

fn flash<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, APath<'x>>,
) -> (Vec<DemoEffect>, Completed<APath<'x>>) {
    (vec![DemoEffect::FlashOverlay], st.complete())
}

/// Reads the timer id while B is still there.
const fn snap_return_home(_ev: &KeyEvent, a: &APath<'_>) -> TimerId {
    a.b.return_home.id
}

/// Rearms while B is standing; cancels from the snap when invalidated.
fn return_home_deadline<'x>(
    _ev: &KeyEvent,
    snapped: TimerId,
    st: AscendState<'_, APath<'x>>,
) -> (Vec<DemoEffect>, Completed<APath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(a) => {
            let fresh = TimerId::fresh();
            a.b.return_home = TimerGuard { id: fresh };
            (
                vec![
                    DemoEffect::CancelTimer(snapped),
                    DemoEffect::ScheduleTimer(fresh),
                ],
                a.complete(),
            )
        }
        MaybeInvalidated::Invalidated(c) => (vec![DemoEffect::CancelTimer(snapped)], c),
    }
}

const fn demo(id: u64) -> A {
    A {
        b: B {
            return_home: TimerGuard { id: TimerId(id) },
        },
    }
}

#[test]
fn key_h_leaves_and_the_post_cancels_from_its_snap() {
    let mut a = demo(7);
    assert_eq!(
        dispatch::<M, A, _>(&mut a, &key("h")),
        vec![DemoEffect::CancelTimer(TimerId(7))]
    );
    assert_eq!(
        a.b.return_home.id,
        TimerId(7),
        "the invalidated arm rearms nothing"
    );
}

#[test]
fn an_unclaimed_key_still_rearms() {
    let mut a = demo(7);
    assert_eq!(
        dispatch::<M, A, _>(&mut a, &key("x")),
        vec![
            DemoEffect::CancelTimer(TimerId(7)),
            DemoEffect::ScheduleTimer(TimerId(1)),
        ]
    );
    assert_eq!(a.b.return_home.id, TimerId(1), "the post rearmed it");
}

#[test]
fn a_post_and_a_bind_both_run_in_source_order() {
    let mut a = demo(7);
    assert_eq!(
        dispatch::<M, A, _>(&mut a, &key("esc")),
        vec![
            DemoEffect::CancelTimer(TimerId(7)),
            DemoEffect::ScheduleTimer(TimerId(1)),
            DemoEffect::FlashOverlay,
        ]
    );
}

#[test]
fn a_pre_snaps_before_the_descent_mutates() {
    let mut a = demo(7);
    let effects = dispatch::<M, A, _>(&mut a, &key("bump"));
    assert_eq!(
        effects,
        vec![
            DemoEffect::CancelTimer(TimerId(7)),
            DemoEffect::ScheduleTimer(TimerId(1)),
        ],
        "the snap is the id from before the descent, not the 99 it wrote"
    );
    assert_eq!(a.b.return_home.id, TimerId(1));
}

#[derive(Bind)]
#[node(root)]
#[binds(M)]
#[bind(Key("t") => trap_root)]
pub struct Trap {
    pub open: bool,
    #[child]
    pub child: TrapChild,
}

#[derive(Bind)]
#[node(parent_path = TrapPath)]
#[binds(M)]
#[bind(|p: &TrapChildPath| p.parent().open.then_some(Key("t")) => trap_child)]
pub struct TrapChild;

pub type TrapPath<'a> = &'a mut Trap;
pub type TrapChildPath<'a> = PathMut<TrapChild, TrapPath<'a>>;

fn trap_root<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, TrapPath<'x>>,
) -> (Vec<DemoEffect>, Completed<TrapPath<'x>>) {
    (vec![DemoEffect::FlashOverlay], st.complete())
}

fn trap_child<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, TrapChildPath<'x>>,
) -> (Vec<DemoEffect>, Completed<TrapChildPath<'x>>) {
    (vec![DemoEffect::SawStanding], st.complete())
}

#[test]
fn the_deepest_binding_takes_the_claim_and_the_ancestors_is_skipped() {
    let mut trap = Trap {
        open: true,
        child: TrapChild,
    };
    assert_eq!(
        dispatch::<M, Trap, _>(&mut trap, &key("t")),
        vec![DemoEffect::SawStanding],
        "the child claimed, so the root's bind completed where it stood"
    );

    let mut trap = Trap {
        open: false,
        child: TrapChild,
    };
    assert_eq!(
        dispatch::<M, Trap, _>(&mut trap, &key("t")),
        vec![DemoEffect::FlashOverlay]
    );
}

#[derive(Bind)]
#[node(root)]
#[binds(M)]
#[post(AnyKey => witness)]
#[post(AnyKey => witness)]
pub struct Top {
    #[child]
    pub mid: Mid,
}

#[derive(Bind)]
#[node(parent_path = TopPath)]
#[binds(M)]
pub struct Mid {
    #[child]
    pub leaf: Leaf,
}

#[derive(Bind)]
#[node(parent_path = MidPath)]
#[binds(M)]
#[bind(
    Key("go") => if_not_invalidated(leaf_home),
    Key("pair") => if_not_invalidated(and!(emits_flash, emits_cancel)),
    Key("nest") => if_not_invalidated(and!(emits_flash, emits_cancel, emits_flash)),
    Key("leave-then-look") => if_not_invalidated(and!(leaf_home, witness_leaf)),
)]
pub struct Leaf;

pub type TopPath<'a> = &'a mut Top;
pub type MidPath<'a> = PathMut<Mid, TopPath<'a>>;
pub type LeafPath<'a> = PathMut<Leaf, MidPath<'a>>;

fn emits_flash<P: HasStop + CompletesTo<P>>(
    _ev: &KeyEvent,
    _snap: (),
    p: P,
) -> (Vec<DemoEffect>, Completed<P>) {
    (vec![DemoEffect::FlashOverlay], p.complete())
}

fn emits_cancel<P: HasStop + CompletesTo<P>>(
    _ev: &KeyEvent,
    _snap: (),
    p: P,
) -> (Vec<DemoEffect>, Completed<P>) {
    (vec![DemoEffect::CancelTimer(TimerId(0))], p.complete())
}

fn witness_leaf<'x>(
    _ev: &KeyEvent,
    _snap: (),
    p: LeafPath<'x>,
) -> (Vec<DemoEffect>, Completed<LeafPath<'x>>) {
    (vec![DemoEffect::SawStanding], p.complete())
}

fn leaf_home<'x, P>(_ev: &KeyEvent, _snap: (), p: P) -> (Vec<DemoEffect>, Completed<P>)
where
    P: HasStop + IntoAncestor<TopPath<'x>>,
    TopPath<'x>: CompletesTo<P>,
{
    let root: TopPath<'x> = p.into_ancestor();
    (vec![], root.complete())
}

fn witness<'x>(
    _ev: &KeyEvent,
    _snap: (),
    st: AscendState<'_, TopPath<'x>>,
) -> (Vec<DemoEffect>, Completed<TopPath<'x>>) {
    match st.state {
        MaybeInvalidated::NotInvalidated(top) => (vec![DemoEffect::SawStanding], top.complete()),
        MaybeInvalidated::Invalidated(c) => (vec![DemoEffect::SawInvalidated], c),
    }
}

#[test]
fn a_leave_forwards_through_a_node_that_binds_nothing() {
    let mut top = Top {
        mid: Mid { leaf: Leaf },
    };
    assert_eq!(
        dispatch::<M, Top, _>(&mut top, &key("go")),
        vec![DemoEffect::SawInvalidated, DemoEffect::SawStanding]
    );
}

#[test]
fn posts_run_without_a_claim_on_the_standing_branch() {
    let mut top = Top {
        mid: Mid { leaf: Leaf },
    };
    assert_eq!(
        dispatch::<M, Top, _>(&mut top, &key("nothing-binds-this")),
        vec![DemoEffect::SawStanding, DemoEffect::SawStanding]
    );
}

#[test]
fn and_concatenates_effects_in_order() {
    let mut top = Top {
        mid: Mid { leaf: Leaf },
    };
    assert_eq!(
        dispatch::<M, Top, _>(&mut top, &key("pair")),
        vec![
            DemoEffect::FlashOverlay,
            DemoEffect::CancelTimer(TimerId(0)),
            DemoEffect::SawStanding,
            DemoEffect::SawStanding,
        ]
    );
}

#[test]
fn a_leave_ends_the_chain() {
    let mut top = Top {
        mid: Mid { leaf: Leaf },
    };
    assert_eq!(
        dispatch::<M, Top, _>(&mut top, &key("leave-then-look")),
        vec![DemoEffect::SawInvalidated, DemoEffect::SawStanding,]
    );
}

#[test]
fn and_nests() {
    let mut top = Top {
        mid: Mid { leaf: Leaf },
    };
    assert_eq!(
        dispatch::<M, Top, _>(&mut top, &key("nest")),
        vec![
            DemoEffect::FlashOverlay,
            DemoEffect::CancelTimer(TimerId(0)),
            DemoEffect::FlashOverlay,
            DemoEffect::SawStanding,
            DemoEffect::SawStanding,
        ]
    );
}
