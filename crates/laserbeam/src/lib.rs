//! A mutable typed cursor into a single-owner tree. A [`PathMut`] holds a parent and a projection down to a child, with exactly one live `&mut` at a time.
//!
//! ```
//! use laserbeam::PathMut;
//!
//! struct Album { title: String }
//! let mut album = Album { title: "A Night at the Opera".to_string() };
//!
//! let mut path: PathMut<String, &mut Album> = PathMut::from_fn(&mut album, |a| &mut a.title, |a| &a.title);
//! path.get_mut().push_str(" (Remastered)");
//! drop(path);
//!
//! assert_eq!(album.title, "A Night at the Opera (Remastered)");
//! ```

/// Mutable projection from parent to focused node. `Bare` is a function pointer (what the derive emits). `Dyn` is a boxed closure that can capture.
enum ProjMut<Node, Parent> {
    Bare(fn(&mut Parent) -> &mut Node),
    Dyn(Box<dyn for<'p> Fn(&'p mut Parent) -> &'p mut Node>),
}

impl<Node, Parent> ProjMut<Node, Parent> {
    fn apply<'p>(&self, parent: &'p mut Parent) -> &'p mut Node {
        match self {
            Self::Bare(f) => f(parent),
            Self::Dyn(f) => f(parent),
        }
    }
}

/// Shared projection from parent to focused node. Stored beside [`ProjMut`] because applying that one needs `&mut Parent`, which a shared borrow of the path cannot produce.
enum ProjRef<Node, Parent> {
    Bare(fn(&Parent) -> &Node),
    Dyn(Box<dyn for<'p> Fn(&'p Parent) -> &'p Node>),
}

impl<Node, Parent> ProjRef<Node, Parent> {
    fn apply<'p>(&self, parent: &'p Parent) -> &'p Node {
        match self {
            Self::Bare(f) => f(parent),
            Self::Dyn(f) => f(parent),
        }
    }
}

/// Typed mutable path to a `Node`: owned `Parent` plus the projection that re-derives `Node`.
///
/// `parent` is private; the only way up is [`into_parent`](PathMut::into_parent), which consumes the path. [`get_mut`](PathMut::get_mut) borrows the whole path, so holding the leaf and walking up at once does not compile.
///
/// ```compile_fail
/// use laserbeam::PathMut;
/// let mut root = 0_u32;
/// let mut path: PathMut<u32, &mut u32> = PathMut::from_fn(&mut root, |r| &mut **r, |r| &**r);
/// let leaf = path.get_mut();
/// let parent = path.into_parent(); // moves `path` while `leaf` still borrows it
/// let _ = (leaf, parent);
/// ```
///
/// ```compile_fail
/// use laserbeam::PathMut;
/// let mut root = 0_u32;
/// let mut path: PathMut<u32, &mut u32> = PathMut::from_fn(&mut root, |r| &mut **r, |r| &**r);
/// let _parent = path.into_parent();
/// let _leaf = path.get_mut(); // `path` has already been moved
/// ```
///
/// ```compile_fail
/// use laserbeam::PathMut;
/// let mut root = 0_u32;
/// let path: PathMut<u32, &mut u32> = PathMut::from_fn(&mut root, |r| &mut **r, |r| &**r);
/// let _ = path.parent; // private field
/// ```
pub struct PathMut<Node, Parent> {
    parent: Parent,
    projection: ProjMut<Node, Parent>,
    shared: ProjRef<Node, Parent>,
}

impl<Node, Parent> PathMut<Node, Parent> {
    /// Parent plus two non-capturing projections (write and read). Nothing checks that they address the same node.
    #[must_use]
    pub const fn from_fn(
        parent: Parent,
        projection: fn(&mut Parent) -> &mut Node,
        shared: fn(&Parent) -> &Node,
    ) -> Self {
        Self {
            parent,
            projection: ProjMut::Bare(projection),
            shared: ProjRef::Bare(shared),
        }
    }

    /// Parent plus boxed, possibly capturing, projections. Nothing checks that they address the same node.
    #[must_use]
    pub fn from_box(
        parent: Parent,
        projection: Box<dyn for<'p> Fn(&'p mut Parent) -> &'p mut Node>,
        shared: Box<dyn for<'p> Fn(&'p Parent) -> &'p Node>,
    ) -> Self {
        Self {
            parent,
            projection: ProjMut::Dyn(projection),
            shared: ProjRef::Dyn(shared),
        }
    }

    /// Shared reference to the focused node. Takes `&self`, so it composes with [`parent`](Self::parent).
    #[must_use]
    pub fn get(&self) -> &Node {
        self.shared.apply(&self.parent)
    }

    #[must_use]
    pub fn get_mut(&mut self) -> &mut Node {
        self.projection.apply(&mut self.parent)
    }

    #[must_use]
    pub const fn parent(&self) -> &Parent {
        &self.parent
    }

    #[must_use]
    pub fn into_parent(self) -> Parent {
        self.parent
    }
}

#[cfg(test)]
mod tests {
    use super::PathMut;

    struct Sheer {
        heart: Attack,
    }
    struct Attack {
        length: u32,
    }

    #[test]
    fn from_fn_get_mut_into_parent() {
        let mut album = Sheer {
            heart: Attack { length: 1 },
        };
        let mut path: PathMut<Attack, &mut Sheer> =
            PathMut::from_fn(&mut album, |a| &mut a.heart, |a| &a.heart);
        path.get_mut().length = 42;
        let recovered = path.into_parent();
        assert_eq!(recovered.heart.length, 42);
    }

    #[test]
    fn parent_reads_without_consuming() {
        let mut album = Sheer {
            heart: Attack { length: 7 },
        };
        let path: PathMut<Attack, &mut Sheer> =
            PathMut::from_fn(&mut album, |a| &mut a.heart, |a| &a.heart);
        assert_eq!(path.parent().heart.length, 7);
        assert_eq!(path.parent().heart.length, 7);
    }

    #[test]
    fn from_box_can_capture() {
        let mut setlist = vec![10_u32, 20, 30];
        let index = 1_usize;
        {
            let mut path: PathMut<u32, &mut Vec<u32>> = PathMut::from_box(
                &mut setlist,
                Box::new(move |v: &mut &mut Vec<u32>| &mut v[index]),
                Box::new(move |v: &&mut Vec<u32>| &v[index]),
            );
            assert_eq!(*path.get(), 20);
            *path.get_mut() += 5;
        }
        assert_eq!(setlist[1], 25);
    }

    #[test]
    fn ancestor_reads_by_shared_ref() {
        type Outer<'a> = PathMut<Attack, &'a mut Sheer>;
        let mut album = Sheer {
            heart: Attack { length: 7 },
        };
        let outer: Outer = PathMut::from_fn(&mut album, |a| &mut a.heart, |a| &a.heart);
        let mut inner: PathMut<u32, Outer> =
            PathMut::from_fn(outer, |p| &mut p.get_mut().length, |p| &p.get().length);

        let attack: &Outer = inner.ancestor::<Outer>();
        assert_eq!(attack.get().length, 7);

        *inner.get_mut() += 1;
        assert_eq!(*inner.get(), 8);
    }
}

/// Walk up a path to an ancestor by shared reference, keeping the original path.
///
/// One impl per depth, to twelve. They cannot overlap: unifying two depths would need a type that contains itself. Not for multi-parent trees; those use a route enum, so the walk is not unique.
pub trait HasAncestor<Target> {
    fn ancestor(&self) -> &Target;
}

/// Consuming walk to an ancestor. Supertrait of [`HasAncestor`], so one bound gives both.
pub trait IntoAncestor<Target>: HasAncestor<Target> {
    fn into_ancestor(self) -> Target;
}

impl<T> HasAncestor<T> for T {
    fn ancestor(&self) -> &T {
        self
    }
}

impl<T> IntoAncestor<T> for T {
    fn into_ancestor(self) -> T {
        self
    }
}

impl<Node, Parent> PathMut<Node, Parent> {
    #[must_use]
    pub fn ancestor<Target>(&self) -> &Target
    where
        Self: HasAncestor<Target>,
    {
        HasAncestor::ancestor(self)
    }

    /// Consuming walk to `Target`, naming it on the right. The trait method takes no generic arguments, so `path.into_ancestor::<T>()` has to land here.
    #[must_use]
    pub fn into_ancestor<Target>(self) -> Target
    where
        Self: IntoAncestor<Target>,
    {
        IntoAncestor::into_ancestor(self)
    }
}

/// `PathMut<N0, PathMut<N1, .. T>>`. The terminal is any type so a nest can end in a path alias.
macro_rules! path_nest {
    ($t:ty) => { $t };
    ($t:ty, $head:ident $(, $rest:ident)*) => {
        PathMut<$head, path_nest!($t $(, $rest)*)>
    };
}

macro_rules! into_parent_chain {
    ($e:expr) => { $e };
    ($e:expr, $head:ident $(, $rest:ident)*) => {
        into_parent_chain!($e.into_parent() $(, $rest)*)
    };
}

macro_rules! parent_chain {
    ($e:expr) => { $e };
    ($e:expr, $head:ident $(, $rest:ident)*) => {
        parent_chain!($e.parent() $(, $rest)*)
    };
}

macro_rules! ancestor_impls {
    ([$($acc:ident),*]) => {};
    ([$($acc:ident),*], $head:ident $(, $rest:ident)*) => {
        impl<T, $($acc,)* $head> HasAncestor<T> for path_nest!(T $(, $acc)*, $head) {
            fn ancestor(&self) -> &T {
                parent_chain!(self $(, $acc)*, $head)
            }
        }
        impl<T, $($acc,)* $head> IntoAncestor<T> for path_nest!(T $(, $acc)*, $head) {
            fn into_ancestor(self) -> T {
                into_parent_chain!(self $(, $acc)*, $head)
            }
        }
        ancestor_impls!([$($acc,)* $head] $(, $rest)*);
    };
}

ancestor_impls!([], N0, N1, N2, N3, N4, N5, N6, N7, N8, N9, N10, N11);

/// Where a leave stopped: here, or further up. No derives; paths are neither `Debug` nor `PartialEq`.
pub enum Stop<H, U> {
    Here(H),
    Up(U),
}

/// Child of the root: `Up` is the bare root path.
impl<'a, N, R> Stop<PathMut<N, &'a mut R>, &'a mut R> {
    #[must_use]
    pub fn to_maybe_invalidated(self) -> MaybeInvalidated<&'a mut R> {
        match self {
            Self::Here(child) => MaybeInvalidated::NotInvalidated(child.into_parent()),
            Self::Up(root) => MaybeInvalidated::Invalidated(root.complete()),
        }
    }
}

/// Child of a non-root: `Up` is the parent's own leave.
impl<N, N2, Q: Above> Stop<PathMut<N, PathMut<N2, Q>>, Completed<PathMut<N2, Q>>> {
    #[must_use]
    pub fn to_maybe_invalidated(self) -> MaybeInvalidated<PathMut<N2, Q>> {
        match self {
            Self::Here(child) => MaybeInvalidated::NotInvalidated(child.into_parent()),
            Self::Up(rest) => MaybeInvalidated::Invalidated(rest),
        }
    }
}

/// Payload of `Stop::Up` for this path: the root itself, or a `Completed` from this path.
pub trait Above {
    type Up;
}

impl<'a, R> Above for &'a mut R {
    type Up = &'a mut R;
}

impl<N, P: Above> Above for PathMut<N, P> {
    type Up = Completed<Self>;
}

/// A node's path type. Place nodes implement this; a derived level does not, because it has no path.
pub trait HasPath {
    type Path<'a>
    where
        Self: 'a;
}

/// A path's stop layer. A root path can only stop at itself, so its layer is the bare path.
pub trait HasStop: Sized {
    type Stop;

    fn to_maybe_invalidated(completed: Completed<Self>) -> MaybeInvalidated<Self>;
}

impl<N, P: Above> HasStop for PathMut<N, P> {
    type Stop = Stop<Self, P::Up>;

    fn to_maybe_invalidated(completed: Completed<Self>) -> MaybeInvalidated<Self> {
        match completed.into_inner() {
            Stop::Here(path) => MaybeInvalidated::NotInvalidated(path),
            Stop::Up(rest) => MaybeInvalidated::Invalidated(Completed::up(rest)),
        }
    }
}

impl<'a, R> HasStop for &'a mut R {
    type Stop = &'a mut R;

    fn to_maybe_invalidated(completed: Completed<Self>) -> MaybeInvalidated<Self> {
        MaybeInvalidated::NotInvalidated(completed.into_inner())
    }
}

/// A node's own path after descent: still standing, or replaced by a leave that peeled past it. No derives; paths are neither `Debug` nor `PartialEq`.
pub enum MaybeInvalidated<P: HasStop> {
    NotInvalidated(P),
    /// Leave that peeled past this node. A [`Stop::Here`] inside means the path is recoverable.
    Invalidated(Completed<P>),
}

impl<P: HasStop + CompletesTo<P>> MaybeInvalidated<P> {
    #[must_use]
    pub fn complete(self) -> Completed<P> {
        match self {
            Self::NotInvalidated(path) => path.complete(),
            Self::Invalidated(completed) => completed,
        }
    }
}

impl<P: HasStop + CompletesTo<P>> MaybeInvalidated<P>
where
    Completed<P>: TryIntoAncestor<P>,
{
    /// Run `f` with this node's path if the path can still be built. A standing path is lent. A leave that stopped here is recovered and the node stays invalidated. A leave that went above skips `f`.
    #[must_use]
    pub fn descend(self, f: impl FnOnce(P) -> Self) -> Self {
        match self {
            Self::NotInvalidated(p) => f(p),
            Self::Invalidated(completed) => match completed.try_into_ancestor() {
                Ok(p) => Self::Invalidated(f(p).complete()),
                Err(completed) => Self::Invalidated(completed),
            },
        }
    }
}

impl<P: HasStop> MaybeInvalidated<P> {
    #[must_use]
    pub fn ancestor<Target>(&self) -> &Target
    where
        Self: HasAncestor<Target>,
    {
        HasAncestor::ancestor(self)
    }

    #[must_use]
    pub fn into_ancestor<Target>(self) -> Target
    where
        Self: IntoAncestor<Target>,
    {
        IntoAncestor::into_ancestor(self)
    }

    /// # Errors
    ///
    /// The leave this state holds went above `Target`; the state comes back so the caller can forward it.
    pub fn try_into_ancestor<Target>(self) -> Result<Target, Self>
    where
        Self: TryIntoAncestor<Target>,
    {
        TryIntoAncestor::try_into_ancestor(self)
    }
}

/// Both branches of the state hold the root, so a handler that ends at the root does not match on the state.
impl<'a, R, P> HasAncestor<&'a mut R> for MaybeInvalidated<P>
where
    P: HasStop + HasAncestor<&'a mut R>,
    Completed<P>: HasAncestor<&'a mut R>,
{
    fn ancestor(&self) -> &&'a mut R {
        match self {
            Self::NotInvalidated(path) => HasAncestor::ancestor(path),
            Self::Invalidated(completed) => HasAncestor::ancestor(completed),
        }
    }
}

impl<'a, R, P> IntoAncestor<&'a mut R> for MaybeInvalidated<P>
where
    P: HasStop + IntoAncestor<&'a mut R>,
    Completed<P>: IntoAncestor<&'a mut R>,
{
    fn into_ancestor(self) -> &'a mut R {
        match self {
            Self::NotInvalidated(path) => IntoAncestor::into_ancestor(path),
            Self::Invalidated(completed) => IntoAncestor::into_ancestor(completed),
        }
    }
}

/// Reach an ancestor a leave may have destroyed. `Err` returns `self` unchanged so the caller can still forward the leave. The root is always `Ok`.
pub trait TryIntoAncestor<Target>: Sized {
    /// # Errors
    ///
    /// The leave went above `Target`; the value comes back so the caller can forward it.
    fn try_into_ancestor(self) -> Result<Target, Self>;
}

/// Distance zero: the leave reaches its own origin iff it stopped there.
impl<T: HasStop> TryIntoAncestor<T> for Completed<T> {
    fn try_into_ancestor(self) -> Result<T, Self> {
        match self.to_maybe_invalidated() {
            MaybeInvalidated::NotInvalidated(path) => Ok(path),
            MaybeInvalidated::Invalidated(completed) => Err(completed),
        }
    }
}

/// Distance one to the root: always `Ok`.
impl<'a, R, H> TryIntoAncestor<&'a mut R> for Completed<PathMut<H, &'a mut R>> {
    fn try_into_ancestor(self) -> Result<&'a mut R, Self> {
        match self.stop {
            Stop::Here(path) => Ok(path.into_parent()),
            Stop::Up(root) => Ok(root),
        }
    }
}

/// Distance one to a non-root ancestor: `Ok` iff the leave stopped at or below it.
impl<H, N2, Q: Above> TryIntoAncestor<PathMut<N2, Q>> for Completed<PathMut<H, PathMut<N2, Q>>> {
    fn try_into_ancestor(self) -> Result<PathMut<N2, Q>, Self> {
        match self.stop {
            Stop::Here(path) => Ok(path.into_parent()),
            Stop::Up(up) => up.try_into_ancestor().map_err(Self::up),
        }
    }
}

/// One impl per distance of two or more: `Here` walks the standing path up; `Up` asks the parent's leave.
macro_rules! try_into_ancestor_impls {
    ($head:ident) => {};
    ($head:ident, $next:ident $(, $rest:ident)*) => {
        impl<T, $head, $next $(, $rest)*> TryIntoAncestor<T>
            for Completed<path_nest!(T, $head, $next $(, $rest)*)>
        where
            T: Above,
            Completed<path_nest!(T, $next $(, $rest)*)>: TryIntoAncestor<T>,
        {
            fn try_into_ancestor(self) -> Result<T, Self> {
                match self.stop {
                    Stop::Here(path) => Ok(path.into_ancestor()),
                    Stop::Up(up) => up.try_into_ancestor().map_err(Completed::up),
                }
            }
        }
        try_into_ancestor_impls!($next $(, $rest)*);
    };
}

try_into_ancestor_impls!(M1, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, M12);

impl<P, T> TryIntoAncestor<T> for MaybeInvalidated<P>
where
    P: HasStop + IntoAncestor<T>,
    Completed<P>: TryIntoAncestor<T>,
{
    fn try_into_ancestor(self) -> Result<T, Self> {
        match self {
            Self::NotInvalidated(path) => Ok(path.into_ancestor()),
            Self::Invalidated(completed) => {
                TryIntoAncestor::try_into_ancestor(completed).map_err(Self::Invalidated)
            }
        }
    }
}

/// Completed leave from origin `P`. Constructed by [`CompletesTo::complete`] and [`Completed::up`]; `new` is private.
pub struct Completed<P: HasStop> {
    stop: P::Stop,
}

impl<P: HasStop> Completed<P> {
    const fn new(stop: P::Stop) -> Self {
        Self { stop }
    }

    #[must_use]
    pub fn into_inner(self) -> P::Stop {
        self.stop
    }

    #[must_use]
    pub fn to_maybe_invalidated(self) -> MaybeInvalidated<P> {
        P::to_maybe_invalidated(self)
    }

    #[must_use]
    pub fn ancestor<Target>(&self) -> &Target
    where
        Self: HasAncestor<Target>,
    {
        HasAncestor::ancestor(self)
    }

    #[must_use]
    pub fn into_ancestor<Target>(self) -> Target
    where
        Self: IntoAncestor<Target>,
    {
        IntoAncestor::into_ancestor(self)
    }

    /// # Errors
    ///
    /// The leave went above `Target`; it comes back so the caller can forward it.
    pub fn try_into_ancestor<Target>(self) -> Result<Target, Self>
    where
        Self: TryIntoAncestor<Target>,
    {
        TryIntoAncestor::try_into_ancestor(self)
    }
}

impl<'a, R> HasAncestor<&'a mut R> for Completed<&'a mut R> {
    fn ancestor(&self) -> &&'a mut R {
        &self.stop
    }
}

impl<'a, R> IntoAncestor<&'a mut R> for Completed<&'a mut R> {
    fn into_ancestor(self) -> &'a mut R {
        self.stop
    }
}

/// A leave holds the root on every inhabitant. Shallower ancestors may be gone; those are [`TryIntoAncestor`].
impl<'a, R, N, P> HasAncestor<&'a mut R> for Completed<PathMut<N, P>>
where
    P: Above,
    PathMut<N, P>: HasAncestor<&'a mut R>,
    P::Up: HasAncestor<&'a mut R>,
{
    fn ancestor(&self) -> &&'a mut R {
        match &self.stop {
            Stop::Here(path) => path.ancestor(),
            Stop::Up(rest) => HasAncestor::ancestor(rest),
        }
    }
}

impl<'a, R, N, P> IntoAncestor<&'a mut R> for Completed<PathMut<N, P>>
where
    P: Above,
    PathMut<N, P>: IntoAncestor<&'a mut R>,
    P::Up: IntoAncestor<&'a mut R>,
{
    fn into_ancestor(self) -> &'a mut R {
        match self.stop {
            Stop::Here(path) => path.into_ancestor(),
            Stop::Up(rest) => IntoAncestor::into_ancestor(rest),
        }
    }
}

/// Normalize a bare root path into a leave from the root.
impl<'a, R> From<&'a mut R> for Completed<&'a mut R> {
    fn from(root: &'a mut R) -> Self {
        Self::new(root)
    }
}

impl<N, Par: Above> Completed<PathMut<N, Par>> {
    /// Inverse of unwrapping one `Up` level. A parent that inspected its child's leave and must still return its own `Completed` uses this.
    #[must_use]
    pub const fn up(above: Par::Up) -> Self {
        Self::new(Stop::Up(above))
    }
}

/// Complete a leave from origin `O` at this path.
///
/// `O` is a type parameter because one focus completes into every `Completed` whose chain contains it. The call site's expected type pins `O`. Impls are per peel distance; off-chain completes do not compile.
pub trait CompletesTo<O: HasStop> {
    fn complete(self) -> Completed<O>;
}

macro_rules! up_wrap {
    ($e:expr) => { $e };
    ($e:expr, $head:ident $(, $rest:ident)*) => {
        Completed::new(Stop::Up(up_wrap!($e $(, $rest)*)))
    };
}

impl<N, P: Above> CompletesTo<Self> for PathMut<N, P> {
    fn complete(self) -> Completed<Self> {
        Completed::new(Stop::Here(self))
    }
}

impl<'a, R> CompletesTo<&'a mut R> for &'a mut R {
    fn complete(self) -> Completed<&'a mut R> {
        Completed::new(self)
    }
}

/// Two `CompletesTo` impls per peel distance: focus still a path, and focus at the root.
macro_rules! complete_impls {
    ([$($done:ident),*]) => {};
    ([$($done:ident),*], $head:ident $(, $rest:ident)*) => {
        impl<$($done,)* $head, N, P: Above> CompletesTo<path_nest!(PathMut<N, P>, $($done,)* $head)>
            for PathMut<N, P>
        {
            fn complete(self) -> Completed<path_nest!(PathMut<N, P>, $($done,)* $head)> {
                up_wrap!(Completed::new(Stop::Here(self)), $($done,)* $head)
            }
        }

        impl<'a, R, $($done,)* $head> CompletesTo<path_nest!(&'a mut R, $($done,)* $head)>
            for &'a mut R
        {
            fn complete(self) -> Completed<path_nest!(&'a mut R, $($done,)* $head)> {
                up_wrap!(self, $($done,)* $head)
            }
        }

        complete_impls!([$($done,)* $head] $(, $rest)*);
    };
}

complete_impls!([], N1, N2, N3, N4, N5, N6, N7, N8, N9, N10, N11, N12);

#[cfg(test)]
mod ancestor_tests {
    use crate::{HasAncestor, IntoAncestor, PathMut};

    struct Root;
    struct Target;
    type TargetPath<'a> = PathMut<Target, &'a mut Root>;

    struct N1;
    struct N2;
    struct N3;
    struct N4;
    struct N5;
    struct N6;
    struct N7;
    struct N8;
    struct N9;
    struct N10;
    struct N11;
    struct N12;

    type D1<'a> = PathMut<N1, TargetPath<'a>>;
    type D2<'a> = PathMut<N2, D1<'a>>;
    type D3<'a> = PathMut<N3, D2<'a>>;
    type D4<'a> = PathMut<N4, D3<'a>>;
    type D5<'a> = PathMut<N5, D4<'a>>;
    type D6<'a> = PathMut<N6, D5<'a>>;
    type D7<'a> = PathMut<N7, D6<'a>>;
    type D8<'a> = PathMut<N8, D7<'a>>;
    type D9<'a> = PathMut<N9, D8<'a>>;
    type D10<'a> = PathMut<N10, D9<'a>>;
    type D11<'a> = PathMut<N11, D10<'a>>;
    type D12<'a> = PathMut<N12, D11<'a>>;

    const fn reaches<'a, P: HasAncestor<TargetPath<'a>> + IntoAncestor<TargetPath<'a>>>() {}

    /// Fails to compile if either reach is short.
    #[test]
    fn reaches_from_every_depth_up_to_twelve() {
        reaches::<TargetPath<'_>>();
        reaches::<D1<'_>>();
        reaches::<D2<'_>>();
        reaches::<D6<'_>>();
        reaches::<D11<'_>>();
        reaches::<D12<'_>>();
    }

    #[test]
    fn a_path_reaches_each_of_its_ancestors() {
        const fn to<T, P: HasAncestor<T> + IntoAncestor<T>>() {}
        to::<D2<'_>, D12<'_>>();
        to::<D11<'_>, D12<'_>>();
        to::<TargetPath<'_>, D12<'_>>();
    }
}

#[cfg(test)]
mod complete_tests {
    use crate::{Completed, CompletesTo, HasStop, IntoAncestor, PathMut, Stop};

    struct App {
        hits: u32,
        layer: Layer,
    }
    struct Layer {
        nav: Nav,
    }
    struct Nav {
        hits: u32,
    }

    type AppPath<'a> = &'a mut App;
    type LayerPath<'a> = PathMut<Layer, AppPath<'a>>;
    type NavPath<'a> = PathMut<Nav, LayerPath<'a>>;

    fn tree(nav_hits: u32, app_hits: u32) -> App {
        App {
            hits: app_hits,
            layer: Layer {
                nav: Nav { hits: nav_hits },
            },
        }
    }

    fn layer_path(app: &mut App) -> LayerPath<'_> {
        PathMut::from_fn(app, |a| &mut a.layer, |a| &a.layer)
    }

    fn nav_path(app: &mut App) -> NavPath<'_> {
        PathMut::from_fn(
            layer_path(app),
            |lp| &mut lp.get_mut().nav,
            |lp| &lp.get().nav,
        )
    }

    /// Pins the expanded `Stop` shapes for the three-level tree.
    #[allow(dead_code)]
    fn shapes<'a>(nav: Completed<NavPath<'a>>) {
        let stop: Stop<NavPath<'a>, Completed<LayerPath<'a>>> = nav.into_inner();
        if let Stop::Up(rest) = stop {
            let _: Stop<LayerPath<'a>, AppPath<'a>> = rest.into_inner();
        }
    }

    #[test]
    fn complete_at_nav() {
        let mut app = tree(7, 0);
        let out: Completed<NavPath<'_>> = nav_path(&mut app).complete();
        let Stop::Here(mut nav) = out.into_inner() else {
            panic!("expected Here");
        };
        assert_eq!(nav.get().hits, 7);
        nav.get_mut().hits = 8;
        drop(nav);
        assert_eq!(app.layer.nav.hits, 8);
    }

    #[test]
    fn one_peel() {
        let mut app = tree(7, 0);
        let out: Completed<NavPath<'_>> = nav_path(&mut app).into_parent().complete();
        let Stop::Up(rest) = out.into_inner() else {
            panic!("expected Up");
        };
        let Stop::Here(layer) = rest.into_inner() else {
            panic!("expected Up(Here(layer))");
        };
        assert_eq!(layer.get().nav.hits, 7);
    }

    #[test]
    fn two_peels() {
        let mut app = tree(0, 0);
        {
            let out: Completed<NavPath<'_>> =
                nav_path(&mut app).into_parent().into_parent().complete();
            let Stop::Up(rest) = out.into_inner() else {
                panic!("expected Up");
            };
            let Stop::Up(root) = rest.into_inner() else {
                panic!("expected Up(Up(app))");
            };
            root.hits = 3;
        }
        assert_eq!(app.hits, 3);
    }

    #[test]
    fn layer_origin_bare_root() {
        let mut app = tree(0, 0);
        {
            let out: Completed<LayerPath<'_>> = layer_path(&mut app).into_parent().complete();
            let Stop::Up(root) = out.into_inner() else {
                panic!("expected Up(app)");
            };
            root.hits = 1;
        }
        assert_eq!(app.hits, 1);
    }

    #[test]
    fn root_completes_bare() {
        let mut app = tree(0, 0);
        {
            let out: Completed<AppPath<'_>> = (&mut app).complete();
            let root = out.into_inner();
            root.hits = 5;
        }
        assert_eq!(app.hits, 5);
    }

    fn stay<P: CompletesTo<P> + HasStop>(path: P) -> Completed<P> {
        path.complete()
    }

    fn to_root<'a, P>(path: P) -> Completed<P>
    where
        P: IntoAncestor<AppPath<'a>> + HasStop,
        AppPath<'a>: CompletesTo<P>,
    {
        path.into_ancestor().complete()
    }

    #[test]
    fn same_generic_handler_at_nav_and_root() {
        let mut app = tree(7, 0);

        let stay_nav: Completed<NavPath<'_>> = stay(nav_path(&mut app));
        let Stop::Here(nav) = stay_nav.into_inner() else {
            panic!("stay at nav is Here");
        };
        assert_eq!(nav.get().hits, 7);
        drop(nav);

        let stay_root: Completed<AppPath<'_>> = stay(&mut app);
        assert_eq!(stay_root.into_inner().hits, 0);

        let mut app = tree(0, 0);
        {
            let from_nav: Completed<NavPath<'_>> = to_root(nav_path(&mut app));
            let Stop::Up(rest) = from_nav.into_inner() else {
                panic!("to_root from nav peels");
            };
            let Stop::Up(root) = rest.into_inner() else {
                panic!("two peels to root");
            };
            assert_eq!(root.hits, 0);
        }

        let from_root: Completed<AppPath<'_>> = to_root(&mut app);
        assert_eq!(from_root.into_inner().hits, 0);
    }

    #[test]
    fn all_peel_depths_unify() {
        fn all_depths(nav: NavPath<'_>, branch: u8) -> Completed<NavPath<'_>> {
            match branch {
                0 => nav.complete(),
                1 => nav.into_parent().complete(),
                _ => nav.into_parent().into_parent().complete(),
            }
        }

        let mut app = tree(7, 0);
        {
            let here = all_depths(nav_path(&mut app), 0);
            let Stop::Here(nav) = here.into_inner() else {
                panic!("branch 0");
            };
            assert_eq!(nav.get().hits, 7);
        }
        {
            let one = all_depths(nav_path(&mut app), 1);
            let Stop::Up(rest) = one.into_inner() else {
                panic!("branch 1");
            };
            let Stop::Here(layer) = rest.into_inner() else {
                panic!("branch 1 Here(layer)");
            };
            assert_eq!(layer.get().nav.hits, 7);
        }
        {
            let two = all_depths(nav_path(&mut app), 2);
            let Stop::Up(rest) = two.into_inner() else {
                panic!("branch 2");
            };
            let Stop::Up(root) = rest.into_inner() else {
                panic!("branch 2 Up(app)");
            };
            assert_eq!(root.hits, 0);
        }
    }

    #[test]
    fn parent_returns_up_payload() {
        fn parent_returns_up_payload(child: Completed<NavPath<'_>>) -> Completed<LayerPath<'_>> {
            match child.into_inner() {
                Stop::Here(nav) => nav.into_parent().complete(),
                Stop::Up(rest) => rest,
            }
        }

        let mut app = tree(7, 0);
        let from_here = parent_returns_up_payload(nav_path(&mut app).complete());
        let Stop::Here(layer) = from_here.into_inner() else {
            panic!("Here arm peels to layer");
        };
        assert_eq!(layer.get().nav.hits, 7);

        let mut app = tree(7, 0);
        let from_up = parent_returns_up_payload(nav_path(&mut app).into_parent().complete());
        let Stop::Here(layer) = from_up.into_inner() else {
            panic!("Up arm is the layer Completed");
        };
        assert_eq!(layer.get().nav.hits, 7);

        let mut app = tree(0, 0);
        {
            let from_root = parent_returns_up_payload(
                nav_path(&mut app).into_parent().into_parent().complete(),
            );
            let Stop::Up(root) = from_root.into_inner() else {
                panic!("Up past layer is bare root");
            };
            root.hits = 9;
        }
        assert_eq!(app.hits, 9);
    }

    #[test]
    fn parent_inspects_and_rebuilds_with_up() {
        fn parent_inspect(child: Completed<NavPath<'_>>) -> Completed<LayerPath<'_>> {
            match child.into_inner() {
                Stop::Here(nav) => nav.into_parent().complete(),
                Stop::Up(rest) => match rest.into_inner() {
                    Stop::Here(layer) => layer.complete(),
                    Stop::Up(above) => Completed::up(above),
                },
            }
        }

        let mut app = tree(7, 0);
        let stopped_here = parent_inspect(nav_path(&mut app).into_parent().complete());
        let Stop::Here(layer) = stopped_here.into_inner() else {
            panic!("stopped at layer");
        };
        assert_eq!(layer.get().nav.hits, 7);

        let mut app = tree(0, 0);
        {
            let gone = parent_inspect(nav_path(&mut app).into_parent().into_parent().complete());
            let Stop::Up(root) = gone.into_inner() else {
                panic!("gone above layer");
            };
            root.hits = 4;
        }
        assert_eq!(app.hits, 4);
    }
}

#[cfg(test)]
mod maybe_invalidated_tests {
    use crate::{Completed, CompletesTo, MaybeInvalidated, PathMut, Stop};

    struct App {
        hits: u32,
        layer: Layer,
    }
    struct Layer {
        nav: Nav,
    }
    struct Nav {
        hits: u32,
    }

    type AppPath<'a> = &'a mut App;
    type LayerPath<'a> = PathMut<Layer, AppPath<'a>>;
    type NavPath<'a> = PathMut<Nav, LayerPath<'a>>;

    const fn tree(nav_hits: u32, app_hits: u32) -> App {
        App {
            hits: app_hits,
            layer: Layer {
                nav: Nav { hits: nav_hits },
            },
        }
    }

    fn layer_path(app: &mut App) -> LayerPath<'_> {
        PathMut::from_fn(app, |a| &mut a.layer, |a| &a.layer)
    }

    fn nav_path(app: &mut App) -> NavPath<'_> {
        PathMut::from_fn(
            layer_path(app),
            |lp| &mut lp.get_mut().nav,
            |lp| &lp.get().nav,
        )
    }

    #[test]
    fn a_root_reads_its_childs_leave_as_its_own_state() {
        let mut app = tree(7, 0);
        {
            let stayed: Completed<LayerPath<'_>> = layer_path(&mut app).complete();
            let MaybeInvalidated::NotInvalidated(root) = stayed.into_inner().to_maybe_invalidated()
            else {
                panic!("a child that stopped at itself leaves the root standing");
            };
            root.hits = 1;
        }
        assert_eq!(app.hits, 1);

        {
            let left: Completed<LayerPath<'_>> = layer_path(&mut app).into_parent().complete();
            let MaybeInvalidated::Invalidated(completed) = left.into_inner().to_maybe_invalidated()
            else {
                panic!("a child that went above destroyed the root's descent");
            };
            completed.into_inner().hits = 2;
        }
        assert_eq!(app.hits, 2);
    }

    #[test]
    fn a_layer_reads_its_childs_leave_as_its_own_state() {
        let mut app = tree(7, 0);
        {
            let stayed: Completed<NavPath<'_>> = nav_path(&mut app).complete();
            let MaybeInvalidated::NotInvalidated(layer) =
                stayed.into_inner().to_maybe_invalidated()
            else {
                panic!("nav stopped at itself, so the layer stands");
            };
            assert_eq!(layer.get().nav.hits, 7);
        }

        {
            let left: Completed<NavPath<'_>> = nav_path(&mut app).into_parent().complete();
            let MaybeInvalidated::Invalidated(completed) = left.into_inner().to_maybe_invalidated()
            else {
                panic!("nav left, so the layer's descent is invalidated");
            };
            let Stop::Here(layer) = completed.into_inner() else {
                panic!("the leave stopped at the layer");
            };
            assert_eq!(layer.get().nav.hits, 7);
        }
    }

    #[test]
    fn a_returned_leave_folds_back_into_the_state() {
        let mut app = tree(7, 0);
        {
            let stayed: Completed<LayerPath<'_>> = layer_path(&mut app).complete();
            let MaybeInvalidated::NotInvalidated(layer) = stayed.to_maybe_invalidated() else {
                panic!("stopping here re-establishes the path");
            };
            assert_eq!(layer.get().nav.hits, 7);
        }

        {
            let left: Completed<LayerPath<'_>> = layer_path(&mut app).into_parent().complete();
            let MaybeInvalidated::Invalidated(completed) = left.to_maybe_invalidated() else {
                panic!("going above stays a leave");
            };
            let Stop::Up(root) = completed.into_inner() else {
                panic!("the leave still points above the layer");
            };
            root.hits = 3;
        }
        assert_eq!(app.hits, 3);
    }

    #[test]
    fn a_leave_from_the_root_never_invalidates_it() {
        let mut app = tree(0, 0);
        {
            let folded: Completed<AppPath<'_>> = Completed::from(&mut app);
            let MaybeInvalidated::NotInvalidated(root) = folded.to_maybe_invalidated() else {
                panic!("the root's own leave leaves the root standing");
            };
            root.hits = 4;
        }
        assert_eq!(app.hits, 4);
    }

    #[test]
    fn either_branch_completes() {
        let mut app = tree(7, 0);
        {
            let here: Completed<LayerPath<'_>> =
                MaybeInvalidated::NotInvalidated(layer_path(&mut app)).complete();
            let Stop::Here(layer) = here.into_inner() else {
                panic!("a standing path completes where it stands");
            };
            assert_eq!(layer.get().nav.hits, 7);
        }

        {
            let left: Completed<LayerPath<'_>> = layer_path(&mut app).into_parent().complete();
            let up: Completed<LayerPath<'_>> = MaybeInvalidated::Invalidated(left).complete();
            let Stop::Up(root) = up.into_inner() else {
                panic!("an invalidated state completes to the leave it holds");
            };
            root.hits = 5;
        }
        assert_eq!(app.hits, 5);
    }

    #[test]
    fn descend_lends_a_standing_path() {
        let mut app = tree(7, 0);
        let out = MaybeInvalidated::NotInvalidated(layer_path(&mut app)).descend(|mut layer| {
            layer.get_mut().nav.hits = 8;
            MaybeInvalidated::NotInvalidated(layer)
        });
        let MaybeInvalidated::NotInvalidated(layer) = out else {
            panic!("a standing path stays standing when the sibling stays");
        };
        assert_eq!(layer.get().nav.hits, 8);
    }

    #[test]
    fn descend_recovers_a_stopped_here_leave_and_stays_invalidated() {
        let mut app = tree(7, 0);
        let stopped_here: MaybeInvalidated<LayerPath<'_>> =
            MaybeInvalidated::Invalidated(layer_path(&mut app).complete());
        let out = stopped_here.descend(|mut layer| {
            layer.get_mut().nav.hits = 8;
            MaybeInvalidated::NotInvalidated(layer)
        });
        let MaybeInvalidated::Invalidated(completed) = out else {
            panic!("a stopped-here leave is preserved past the sibling");
        };
        let Stop::Here(layer) = completed.into_inner() else {
            panic!("the preserved leave still stops here");
        };
        assert_eq!(layer.get().nav.hits, 8);
    }

    #[test]
    fn descend_lets_a_higher_leave_replace_a_stopped_here_one() {
        let mut app = tree(0, 0);
        {
            let stopped_here: MaybeInvalidated<LayerPath<'_>> =
                MaybeInvalidated::Invalidated(layer_path(&mut app).complete());
            let out = stopped_here
                .descend(|layer| MaybeInvalidated::Invalidated(layer.into_parent().complete()));
            let MaybeInvalidated::Invalidated(completed) = out else {
                panic!("the higher leave is the state");
            };
            let Stop::Up(root) = completed.into_inner() else {
                panic!("the higher leave went above the layer");
            };
            root.hits = 6;
        }
        assert_eq!(app.hits, 6);
    }

    #[test]
    fn descend_skips_when_the_leave_went_above() {
        let mut app = tree(0, 0);
        {
            let gone: MaybeInvalidated<LayerPath<'_>> =
                MaybeInvalidated::Invalidated(layer_path(&mut app).into_parent().complete());
            let out = gone.descend(|_layer| panic!("the sibling is not descended"));
            let MaybeInvalidated::Invalidated(completed) = out else {
                panic!("the leave forwards");
            };
            let Stop::Up(root) = completed.into_inner() else {
                panic!("the leave still points above the layer");
            };
            root.hits = 9;
        }
        assert_eq!(app.hits, 9);
    }
}

#[cfg(test)]
mod ancestors_through_a_leave_tests {
    use crate::{
        Completed, CompletesTo, HasAncestor, HasStop, IntoAncestor, MaybeInvalidated, PathMut,
    };

    struct App {
        hits: u32,
        layer: Layer,
    }
    struct Layer {
        hits: u32,
        nav: Nav,
    }
    struct Nav {
        hits: u32,
        deep: Deep,
    }
    struct Deep {
        hits: u32,
    }

    type AppPath<'a> = &'a mut App;
    type LayerPath<'a> = PathMut<Layer, AppPath<'a>>;
    type NavPath<'a> = PathMut<Nav, LayerPath<'a>>;
    type DeepPath<'a> = PathMut<Deep, NavPath<'a>>;

    const fn tree() -> App {
        App {
            hits: 0,
            layer: Layer {
                hits: 0,
                nav: Nav {
                    hits: 0,
                    deep: Deep { hits: 0 },
                },
            },
        }
    }

    fn layer_path(app: &mut App) -> LayerPath<'_> {
        PathMut::from_fn(app, |a| &mut a.layer, |a| &a.layer)
    }

    fn nav_path(app: &mut App) -> NavPath<'_> {
        PathMut::from_fn(
            layer_path(app),
            |lp| &mut lp.get_mut().nav,
            |lp| &lp.get().nav,
        )
    }

    fn deep_path(app: &mut App) -> DeepPath<'_> {
        PathMut::from_fn(
            nav_path(app),
            |np| &mut np.get_mut().deep,
            |np| &np.get().deep,
        )
    }

    /// Fails to compile if either reach is short at any depth.
    #[test]
    fn completed_and_state_reach_the_root_at_every_depth() {
        const fn reaches<'a, T: HasAncestor<AppPath<'a>> + IntoAncestor<AppPath<'a>>>() {}
        reaches::<Completed<AppPath<'_>>>();
        reaches::<Completed<LayerPath<'_>>>();
        reaches::<Completed<NavPath<'_>>>();
        reaches::<Completed<DeepPath<'_>>>();
        reaches::<MaybeInvalidated<AppPath<'_>>>();
        reaches::<MaybeInvalidated<LayerPath<'_>>>();
        reaches::<MaybeInvalidated<NavPath<'_>>>();
        reaches::<MaybeInvalidated<DeepPath<'_>>>();
    }

    #[test]
    fn a_leave_holds_the_root_wherever_it_stopped() {
        let mut app = tree();
        {
            let stopped_at_nav: Completed<NavPath<'_>> = nav_path(&mut app).complete();
            assert_eq!(stopped_at_nav.ancestor::<AppPath<'_>>().hits, 0);
            stopped_at_nav.into_ancestor::<AppPath<'_>>().hits = 1;
        }
        assert_eq!(app.hits, 1);

        {
            let stopped_at_layer: Completed<NavPath<'_>> =
                nav_path(&mut app).into_parent().complete();
            stopped_at_layer.into_ancestor::<AppPath<'_>>().hits = 2;
        }
        assert_eq!(app.hits, 2);

        {
            let peeled_to_root: Completed<NavPath<'_>> =
                nav_path(&mut app).into_parent().into_parent().complete();
            peeled_to_root.into_ancestor::<AppPath<'_>>().hits = 3;
        }
        assert_eq!(app.hits, 3);

        {
            let from_deep: Completed<DeepPath<'_>> = deep_path(&mut app).complete();
            assert_eq!(from_deep.ancestor::<AppPath<'_>>().layer.hits, 0);
            assert_eq!(from_deep.ancestor::<AppPath<'_>>().layer.nav.hits, 0);
            assert_eq!(from_deep.ancestor::<AppPath<'_>>().layer.nav.deep.hits, 0);
            from_deep.into_ancestor::<AppPath<'_>>().hits = 4;
        }
        assert_eq!(app.hits, 4);
    }

    #[test]
    fn one_root_handler_serves_both_branches_at_every_depth() {
        fn go_root<'a, P>(state: MaybeInvalidated<P>) -> Completed<P>
        where
            P: HasStop,
            MaybeInvalidated<P>: IntoAncestor<AppPath<'a>>,
            AppPath<'a>: CompletesTo<P>,
        {
            let root: AppPath<'a> = state.into_ancestor();
            root.hits += 1;
            root.complete()
        }

        let mut app = tree();
        {
            let standing = go_root(MaybeInvalidated::NotInvalidated(nav_path(&mut app)));
            assert_eq!(standing.into_ancestor::<AppPath<'_>>().hits, 1);
        }
        {
            let left: Completed<NavPath<'_>> = nav_path(&mut app).into_parent().complete();
            let invalidated = go_root(MaybeInvalidated::Invalidated(left));
            assert_eq!(invalidated.into_ancestor::<AppPath<'_>>().hits, 2);
        }
        {
            let at_the_root = go_root(MaybeInvalidated::NotInvalidated(&mut app));
            assert_eq!(at_the_root.into_ancestor::<AppPath<'_>>().hits, 3);
        }
        assert_eq!(app.hits, 3);
    }

    #[test]
    fn try_at_distance_zero_recovers_a_here_stop() {
        let mut app = tree();
        {
            let stopped: Completed<NavPath<'_>> = nav_path(&mut app).complete();
            let Ok(mut nav) = stopped.try_into_ancestor::<NavPath<'_>>() else {
                panic!("a leave that stopped at nav still holds nav");
            };
            nav.get_mut().hits = 5;
        }
        assert_eq!(app.layer.nav.hits, 5);

        {
            let peeled: Completed<NavPath<'_>> = nav_path(&mut app).into_parent().complete();
            let Err(back) = peeled.try_into_ancestor::<NavPath<'_>>() else {
                panic!("a leave that peeled past nav cannot hand nav back");
            };
            back.into_ancestor::<AppPath<'_>>().hits = 6;
        }
        assert_eq!(app.hits, 6);
    }

    #[test]
    fn try_reaches_a_mid_ancestor_iff_the_leave_stopped_at_or_below_it() {
        let mut app = tree();
        {
            let stopped_below: Completed<NavPath<'_>> = nav_path(&mut app).complete();
            let Ok(mut layer) = stopped_below.try_into_ancestor::<LayerPath<'_>>() else {
                panic!("stopping at nav leaves the layer above it standing");
            };
            layer.get_mut().hits = 1;
        }
        assert_eq!(app.layer.hits, 1);

        {
            let stopped_at: Completed<NavPath<'_>> = nav_path(&mut app).into_parent().complete();
            let Ok(mut layer) = stopped_at.try_into_ancestor::<LayerPath<'_>>() else {
                panic!("stopping exactly at the layer recovers it");
            };
            layer.get_mut().hits = 2;
        }
        assert_eq!(app.layer.hits, 2);

        {
            let peeled: Completed<NavPath<'_>> =
                nav_path(&mut app).into_parent().into_parent().complete();
            let Err(back) = peeled.try_into_ancestor::<LayerPath<'_>>() else {
                panic!("a leave past the layer cannot hand it back");
            };
            back.into_ancestor::<AppPath<'_>>().hits = 3;
        }
        assert_eq!(app.hits, 3);
    }

    #[test]
    fn try_to_the_root_always_succeeds() {
        let mut app = tree();
        {
            let peeled: Completed<LayerPath<'_>> = layer_path(&mut app).into_parent().complete();
            let Ok(root) = peeled.try_into_ancestor::<AppPath<'_>>() else {
                panic!("the root outlives every leave");
            };
            root.hits = 7;
        }
        assert_eq!(app.hits, 7);
    }

    #[test]
    fn try_on_the_state_covers_both_branches() {
        let mut app = tree();
        {
            let standing: MaybeInvalidated<NavPath<'_>> =
                MaybeInvalidated::NotInvalidated(nav_path(&mut app));
            let Ok(mut layer) = standing.try_into_ancestor::<LayerPath<'_>>() else {
                panic!("a standing path reaches its own ancestors");
            };
            layer.get_mut().hits = 8;
        }
        assert_eq!(app.layer.hits, 8);

        {
            let leave: Completed<NavPath<'_>> =
                nav_path(&mut app).into_parent().into_parent().complete();
            let invalidated: MaybeInvalidated<NavPath<'_>> = MaybeInvalidated::Invalidated(leave);
            let Err(back) = invalidated.try_into_ancestor::<LayerPath<'_>>() else {
                panic!("the leave went above the layer");
            };
            let MaybeInvalidated::Invalidated(forwardable) = back else {
                panic!("the state comes back as it went in");
            };
            forwardable.into_ancestor::<AppPath<'_>>().hits = 9;
        }
        assert_eq!(app.hits, 9);
    }

    #[test]
    fn try_here_arm_at_macro_depth() {
        let mut app = tree();
        {
            let stopped_at_deep: Completed<DeepPath<'_>> = deep_path(&mut app).complete();
            let Ok(mut layer) = stopped_at_deep.try_into_ancestor::<LayerPath<'_>>() else {
                panic!("stopping at deep leaves the layer two levels up standing");
            };
            layer.get_mut().hits = 11;
        }
        assert_eq!(app.layer.hits, 11);
    }

    #[test]
    fn try_err_rebuilds_through_the_macro() {
        let mut app = tree();
        {
            let peeled: Completed<DeepPath<'_>> = deep_path(&mut app)
                .into_parent()
                .into_parent()
                .into_parent()
                .complete();
            let Err(back) = peeled.try_into_ancestor::<LayerPath<'_>>() else {
                panic!("a leave peeled to the root cannot hand the layer back");
            };
            back.into_ancestor::<AppPath<'_>>().hits = 12;
        }
        assert_eq!(app.hits, 12);
    }

    #[test]
    fn the_state_reads_the_root_on_both_branches() {
        let mut app = tree();
        app.hits = 4;
        {
            let standing: MaybeInvalidated<NavPath<'_>> =
                MaybeInvalidated::NotInvalidated(nav_path(&mut app));
            assert_eq!(standing.ancestor::<AppPath<'_>>().hits, 4);
        }
        {
            let left: Completed<NavPath<'_>> = nav_path(&mut app).into_parent().complete();
            let invalidated: MaybeInvalidated<NavPath<'_>> = MaybeInvalidated::Invalidated(left);
            assert_eq!(invalidated.ancestor::<AppPath<'_>>().hits, 4);
        }
    }
}
