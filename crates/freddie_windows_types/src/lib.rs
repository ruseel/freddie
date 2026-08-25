//! Events and snapshots from the windows watcher.

/// A running app, by process id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pid(pub i32);

/// A window's `CGWindowID`. Outlives any one `AXUIElement` naming it: elements are
/// created per call, so two for the same window are different pointers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WindowId(pub u32);

/// A rectangle in Accessibility coordinates: origin top-left, y increasing down.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Frame {
    /// Whether `(x, y)` lies in this frame. Half-open: left and top in, right and bottom not.
    #[must_use]
    pub const fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// A monitor. `full` locates a window; `visible` is `full` minus the menu bar and dock.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Monitor {
    pub full: Frame,
    pub visible: Frame,
}

/// Placing a window failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowError {
    /// `watch` was called off the main thread.
    NotMainThread,
    /// The Accessibility permission has not been granted.
    NotTrusted,
    /// The watcher has been dropped.
    NotWatching,
}

impl std::fmt::Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotMainThread => "freddie_windows::watch must run on the main thread",
            Self::NotTrusted => "Accessibility is not granted",
            Self::NotWatching => "not watching windows",
        })
    }
}

impl std::error::Error for WindowError {}

/// One fact the windows watcher can report. Values a fact invalidates are the consumer's reads.
#[derive(Clone, PartialEq, Debug)]
pub enum WindowChange {
    /// A window appeared. Its frame is the consumer's read to make.
    Opened(WindowId),
    /// A window moved. The new frame is the consumer's read to make.
    Moved(WindowId),
    /// A window was resized. Same contract as [`Moved`](Self::Moved).
    Resized(WindowId),
    /// A window went away.
    Closed(WindowId),
    /// Focus changed in the app with this pid. Which window is the consumer's read to make.
    FocusChanged(Pid),
    /// The app and every entry keyed by its pid are gone. Reported after the per-window
    /// [`Closed`](Self::Closed) reports.
    AppGone(Pid),
    /// The monitors changed. Reading `NSScreen` is synchronous in the callback, so the
    /// arrangement rides the event.
    Screens(Vec<Monitor>),
}

/// A placement: the window, where it is, and where to put it.
///
/// `from` orders the writes (grow before move, shrink after).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Placement {
    pub window: WindowId,
    pub from: Frame,
    pub to: Frame,
}
