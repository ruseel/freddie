//! Reports from the display watcher.

/// A display present according to macOS.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Display {
    /// `CGDirectDisplayID`, stable for the life of the connection.
    pub id: DisplayId,
    /// `CGDisplayIsBuiltin`: the laptop's own panel.
    pub builtin: bool,
    /// `NSScreen.localizedName`, which is what `BetterDisplay`'s `-name=` addresses.
    pub name: String,
}

/// A display's id for correlation within a connection session.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DisplayId(pub u32);
