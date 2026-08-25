//! Facts and answers from the selection watcher.

use freddie_windows_types::Pid;

/// What an app's focused element said when asked for its selected text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Selection {
    /// The selected text as the app reports it. Never empty: an empty answer is
    /// [`Empty`](Self::Empty).
    Text(String),
    /// The element answers, and nothing is selected.
    Empty,
    /// No focused element, or the focused element does not expose its selection.
    Unsupported,
}

/// Facts only. The consumer requests the value as a read effect when it hears a fact.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectionChange {
    /// This app's selection changed (or its focus moved). Whatever the consumer knew for
    /// this pid is dead.
    Changed(Pid),
    /// The app is gone: remove the entry.
    AppGone(Pid),
}
