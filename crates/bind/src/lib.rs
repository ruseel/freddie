//! Binding layer over a laserbeam state tree: `#[derive(Bind)]` maps triggers to handlers.
//! Collision checking lives behind the `check` feature.
#![expect(clippy::implicit_hasher)]

#[cfg(feature = "check")]
use std::collections::HashSet;
use std::collections::VecDeque;
use std::hash::Hash;

pub use bind_macro::Bind;

/// One exclusive bind handler per dispatch. The slot is private so a handler cannot un-claim or skip the check.
pub struct Claim<'c> {
    slot: &'c mut Option<()>,
}

impl<'c> Claim<'c> {
    #[must_use]
    pub const fn new(slot: &'c mut Option<()>) -> Self {
        Self { slot }
    }

    #[must_use]
    pub const fn is_taken(&self) -> bool {
        self.slot.is_some()
    }

    pub const fn try_take(&mut self) -> Option<()> {
        if self.slot.is_some() {
            None
        } else {
            *self.slot = Some(());
            Some(())
        }
    }

    /// Fresh `Claim` over the same slot. Each scheduled item takes `Claim` by value; dispatch holds only `&mut Claim`.
    pub const fn reborrow(&mut self) -> Claim<'_> {
        Claim {
            slot: &mut *self.slot,
        }
    }
}

/// Claim and path state handed to a scheduled handler.
pub struct AscendState<'a, P: ::laserbeam::HasStop> {
    claim: Claim<'a>,
    /// Path after descent and every earlier item on this node.
    pub state: ::laserbeam::MaybeInvalidated<P>,
}

impl<'a, P: ::laserbeam::HasStop> AscendState<'a, P> {
    #[must_use]
    pub const fn new(state: ::laserbeam::MaybeInvalidated<P>, claim: Claim<'a>) -> Self {
        Self { claim, state }
    }

    pub const fn claim(&mut self) -> Option<()> {
        self.claim.try_take()
    }

    #[must_use]
    pub fn complete(self) -> ::laserbeam::Completed<P>
    where
        P: ::laserbeam::CompletesTo<P>,
    {
        self.state.complete()
    }
}

/// Adapter so the handler takes the path, not [`AscendState`]. An invalidated path completes with no effects.
pub fn if_not_invalidated<Ev, Snap, P, E, H>(
    handler: H,
) -> impl for<'a> FnOnce(Ev, Snap, AscendState<'a, P>) -> (Vec<E>, ::laserbeam::Completed<P>)
where
    P: ::laserbeam::HasStop,
    H: FnOnce(Ev, Snap, P) -> (Vec<E>, ::laserbeam::Completed<P>),
{
    move |ev, snap, st| match st.state {
        ::laserbeam::MaybeInvalidated::NotInvalidated(p) => handler(ev, snap, p),
        ::laserbeam::MaybeInvalidated::Invalidated(c) => (Vec::new(), c),
    }
}

/// Claim gate for `#[bind]`. Posts do not go through this.
pub fn exclusive<Ev, Snap, P, E, H>(
    handler: H,
) -> impl for<'a> FnOnce(Ev, Snap, AscendState<'a, P>) -> (Vec<E>, ::laserbeam::Completed<P>)
where
    P: ::laserbeam::HasStop + ::laserbeam::CompletesTo<P>,
    H: for<'a> FnOnce(Ev, Snap, AscendState<'a, P>) -> (Vec<E>, ::laserbeam::Completed<P>),
{
    move |ev, snap, mut st| match st.claim() {
        Some(()) => handler(ev, snap, st),
        None => (Vec::new(), st.complete()),
    }
}

/// Runs `a` then `b`. A leave from `a` skips `b`. Does not take the claim; `#[bind]` wraps the whole composition in [`exclusive`]. Both units receive the same event and snap, hence `Copy`.
pub fn and<Ev, Snap, P, E, A, B>(
    a: A,
    b: B,
) -> impl FnOnce(Ev, Snap, P) -> (Vec<E>, ::laserbeam::Completed<P>)
where
    Ev: Copy,
    Snap: Copy,
    P: ::laserbeam::HasStop,
    A: FnOnce(Ev, Snap, P) -> (Vec<E>, ::laserbeam::Completed<P>),
    B: FnOnce(Ev, Snap, P) -> (Vec<E>, ::laserbeam::Completed<P>),
{
    move |ev, snap, p| {
        let (mut effs, completed) = a(ev, snap, p);
        match completed.to_maybe_invalidated() {
            ::laserbeam::MaybeInvalidated::NotInvalidated(p) => {
                let (e, completed) = b(ev, snap, p);
                effs.extend(e);
                (effs, completed)
            }
            ::laserbeam::MaybeInvalidated::Invalidated(completed) => (effs, completed),
        }
    }
}

/// Expands to nested [`and`] calls so each unit keeps its own type.
#[macro_export]
macro_rules! and {
    ($h:expr) => { $h };
    ($h:expr, $($rest:expr),+ $(,)?) => {
        $crate::and($h, $crate::and!($($rest),+))
    };
}

/// Keeps its body when the `check` feature is on. The derive emits this because it cannot see `bind`'s features.
#[cfg(feature = "check")]
#[macro_export]
macro_rules! check_only {
    ($($t:tt)*) => { $($t)* };
}

/// Drops its body when the `check` feature is off.
#[cfg(not(feature = "check"))]
#[macro_export]
macro_rules! check_only {
    ($($t:tt)*) => {};
}

/// Marker type naming an app's trigger, event, and output types.
pub trait Bindings {
    /// Unified trigger enum. Always present even without `check`, because a consumer of `Bindings` cannot see `bind`'s features.
    type Trigger: Eq + Hash;
    type Event;
    /// Effect collection. A handler returns any `IntoIterator` of this; the consumer owns those impls.
    type Output;
}

/// Collects live bind triggers. Takes a path, not `&self`, because a derived-child fn needs one.
#[cfg(feature = "check")]
pub trait AccumulateTriggers<M: Bindings>: HasPath {
    /// # Errors
    ///
    /// Returns [`BindError::DuplicateTrigger`] when a trigger is bound at more
    /// than one node on the active path.
    fn accumulate<'a>(
        path: Self::Path<'a>,
        out: &mut HashSet<M::Trigger>,
    ) -> Result<Self::Path<'a>, BindError>
    where
        Self: 'a;
}

#[cfg(feature = "check")]
#[derive(Debug, PartialEq, Eq)]
pub enum BindError {
    DuplicateTrigger,
    /// The check cannot walk a node with several children.
    MultiChildNode,
}

/// # Errors
///
/// Returns [`BindError::DuplicateTrigger`] when `t` is already in `out`.
#[cfg(feature = "check")]
pub fn insert_or_error<T: Eq + Hash>(out: &mut HashSet<T>, t: T) -> Result<(), BindError> {
    if out.insert(t) {
        Ok(())
    } else {
        Err(BindError::DuplicateTrigger)
    }
}

/// # Errors
///
/// Propagates [`BindError::DuplicateTrigger`] from [`AccumulateTriggers::accumulate`].
#[cfg(feature = "check")]
pub fn accumulate<'a, M, N>(path: N::Path<'a>) -> Result<HashSet<M::Trigger>, BindError>
where
    M: Bindings,
    N: AccumulateTriggers<M> + 'a,
{
    let mut out = HashSet::new();
    <N as AccumulateTriggers<M>>::accumulate(path, &mut out)?;
    Ok(out)
}

pub use ::laserbeam::HasPath;

/// Parent plus immutable data for a level that is not a place in the tree.
pub struct DerivedLevel<Parent, Data> {
    pub parent: Parent,
    pub data: Data,
}

/// Place path at the bottom of a parent chain. A [`DerivedLevel`] flattens to its parent; a place is itself.
pub trait HasTreePath {
    type TreePath;
    fn into_tree_path(self) -> Self::TreePath;
}

impl<R> HasTreePath for &mut R {
    type TreePath = Self;
    fn into_tree_path(self) -> Self {
        self
    }
}

impl<N, P> HasTreePath for ::laserbeam::PathMut<N, P> {
    type TreePath = Self;
    fn into_tree_path(self) -> Self {
        self
    }
}

impl<Parent: HasTreePath, Data> HasTreePath for DerivedLevel<Parent, Data> {
    type TreePath = Parent::TreePath;
    fn into_tree_path(self) -> Parent::TreePath {
        self.parent.into_tree_path()
    }
}

/// Calls a closure trigger. Exists so the closure parameter infers from this signature rather than from an immediate call.
pub fn call_with<S: ?Sized, T>(state: &S, f: impl FnOnce(&S) -> T) -> T {
    f(state)
}

/// Whether this trigger matches a source event. Type matching is `TryFrom<&Event>`; this is the value match.
pub trait EventTrigger {
    type Event;
    #[must_use]
    fn is_matching(&self, event: &Self::Event) -> bool;
}

/// `None` matches nothing. Claims `Option` for every consumer; no crate can add another `EventTrigger` impl for `Option<T>`.
impl<T: EventTrigger> EventTrigger for Option<T> {
    type Event = T::Event;
    fn is_matching(&self, event: &T::Event) -> bool {
        self.as_ref().is_some_and(|t| t.is_matching(event))
    }
}

/// Implements [`EventTrigger`] with `Event = Self` and match by [`PartialEq`]. `$t` must implement [`PartialEq`].
#[macro_export]
macro_rules! self_trigger {
    ($t:ty) => {
        impl $crate::EventTrigger for $t {
            type Event = Self;
            fn is_matching(&self, event: &Self) -> bool {
                self == event
            }
        }
    };
}

/// Dispatch for a derived level, returning a leave at the place path beneath it. Method-position so a caller that cannot name the child's type still finds the impl.
pub trait DispatchIntoTreePath<M: Bindings>: HasTreePath + Sized
where
    Self::TreePath: ::laserbeam::HasStop,
{
    fn dispatch_into_tree_path(
        self,
        event: &M::Event,
        effs: &mut M::Output,
        claim: &mut Claim<'_>,
    ) -> ::laserbeam::Completed<Self::TreePath>;
}

/// Accumulate for a derived level. Separate from [`AccumulateTriggers`] because a derived level has no [`HasPath`].
#[cfg(feature = "check")]
pub trait AccumulateDerivedTriggers<M: Bindings>: Sized {
    /// The level above, handed back after this level's triggers are inserted.
    type Parent;

    /// # Errors
    ///
    /// Returns [`BindError::DuplicateTrigger`] when a trigger is already claimed.
    fn accumulate(self, out: &mut HashSet<M::Trigger>) -> Result<Self::Parent, BindError>;
}

/// Runs the handler the active state binds. Child first, then this node's items, so a child's bind outranks an ancestor's.
pub trait Dispatch<M: Bindings>: HasPath {
    fn dispatch<'a, 'c>(
        path: Self::Path<'a>,
        event: &M::Event,
        effs: &mut M::Output,
        claim: &mut Claim<'c>,
    ) -> ::laserbeam::Completed<Self::Path<'a>>
    where
        Self: 'a,
        Self::Path<'a>: ::laserbeam::HasStop;
}

/// Dispatches `event` from the root. Returns the effects; the caller performs them.
pub fn dispatch<'a, M, N, E>(path: N::Path<'a>, event: &M::Event) -> Vec<E>
where
    M: Bindings<Output = Vec<E>>,
    N: Dispatch<M> + 'a,
    N::Path<'a>: ::laserbeam::HasStop,
{
    let mut effs: Vec<E> = Vec::new();
    let mut claim_slot = None;
    let mut claim = Claim::new(&mut claim_slot);
    let _path = <N as Dispatch<M>>::dispatch(path, event, &mut effs, &mut claim);
    effs
}

/// Synchronous event runner for tests. Drains the queue; an empty queue returns `None`.
pub struct SimpleRunner<'a, M: Bindings, N> {
    root: &'a mut N,
    queue: VecDeque<M::Event>,
}

impl<'a, M, N, E> SimpleRunner<'a, M, N>
where
    M: Bindings<Output = Vec<E>>,
    N: Dispatch<M> + for<'b> HasPath<Path<'b> = &'b mut N>,
{
    pub const fn new(root: &'a mut N) -> Self {
        Self {
            root,
            queue: VecDeque::new(),
        }
    }

    pub fn queue_event(&mut self, event: M::Event) {
        self.queue.push_back(event);
    }

    /// Processes one queued event. `None` if the queue was empty.
    #[expect(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Vec<E>> {
        let event = self.queue.pop_front()?;
        Some(dispatch::<M, N, E>(&mut *self.root, &event))
    }

    /// Queues `event` and processes the front of the queue. That front is `event` only if the queue was empty.
    ///
    /// # Panics
    ///
    /// The `expect` asserts the queue is non-empty after queueing.
    pub fn process_event(&mut self, event: M::Event) -> Vec<E> {
        // Inlined rather than calling `queue_event`/`next`, which the impl's HRTB bound would otherwise force to `'static`.
        self.queue.push_back(event);
        let event = self
            .queue
            .pop_front()
            .expect("the queue is non-empty: an event was just queued");
        dispatch::<M, N, E>(&mut *self.root, &event)
    }
}

impl<M: Bindings, N> SimpleRunner<'_, M, N> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod has_place_tests {
    use super::{DerivedLevel, HasTreePath};
    use laserbeam::PathMut;

    struct Root {
        layer: u32,
    }

    fn path(root: &mut Root) -> PathMut<u32, &mut Root> {
        PathMut::from_fn(root, |r| &mut r.layer, |r| &r.layer)
    }

    #[test]
    fn a_root_path_is_its_own_place() {
        let mut root = Root { layer: 7 };
        let place: &mut Root = HasTreePath::into_tree_path(&mut root);
        place.layer = 8;
        assert_eq!(root.layer, 8);
    }

    #[test]
    fn a_path_mut_is_its_own_place() {
        let mut root = Root { layer: 7 };
        {
            let mut place: PathMut<u32, &mut Root> = HasTreePath::into_tree_path(path(&mut root));
            *place.get_mut() = 9;
        }
        assert_eq!(root.layer, 9);
    }

    #[test]
    fn a_node_flattens_to_its_parent_path() {
        let mut root = Root { layer: 7 };
        {
            let node = DerivedLevel {
                parent: path(&mut root),
                data: "derived",
            };
            let mut place: PathMut<u32, &mut Root> = HasTreePath::into_tree_path(node);
            *place.get_mut() = 10;
        }
        assert_eq!(root.layer, 10);
    }

    #[test]
    fn two_node_layers_flatten_to_the_same_place() {
        let mut root = Root { layer: 7 };
        {
            let node = DerivedLevel {
                parent: DerivedLevel {
                    parent: path(&mut root),
                    data: "outer",
                },
                data: 3_u8,
            };
            let mut place: PathMut<u32, &mut Root> = HasTreePath::into_tree_path(node);
            *place.get_mut() = 11;
        }
        assert_eq!(root.layer, 11);
    }
}
