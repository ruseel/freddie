//! Event sources.

use bind::EventTrigger;
use freddie_keys::KeyEvent;
use freddie_sync::RidingGeneration;
use freddie_windows_types::WindowChange;
use freddie_windows_types::{Frame, Pid, WindowId};

/// Matches every key. Bound at the root as last resort: a key the active layer binds wins.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AnyKey;
impl EventTrigger for AnyKey {
    type Event = KeyEvent;
    fn is_matching(&self, _ev: &KeyEvent) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Foregrounded;

#[cfg_attr(feature = "testing", derive(PartialEq, Eq))]
#[derive(Debug)]
pub struct ForegroundEvent {
    pub app: App,
    /// The process the OS's per-app reports speak; `app` is the keymap's lossy classification.
    pub pid: Pid,
}
impl EventTrigger for Foregrounded {
    type Event = ForegroundEvent;
    fn is_matching(&self, _ev: &ForegroundEvent) -> bool {
        true
    }
}

/// Matches any window change. One root binding records all of them.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Windowed;

#[cfg_attr(feature = "testing", derive(PartialEq))]
#[derive(Debug)]
pub struct WindowEvent {
    pub change: WindowChange,
}
impl EventTrigger for Windowed {
    type Event = WindowEvent;
    fn is_matching(&self, _ev: &WindowEvent) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FrameLanded;

/// A frame read's answer. Only the performer creates one; the wire cannot.
#[cfg_attr(feature = "testing", derive(PartialEq))]
#[derive(Debug)]
pub struct FrameRead {
    pub window: WindowId,
    pub generation: RidingGeneration,
    /// `None` when the read could not answer; the entry stays `Pending`.
    pub frame: Option<Frame>,
}
impl EventTrigger for FrameLanded {
    type Event = FrameRead;
    fn is_matching(&self, _ev: &FrameRead) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FocusLanded;

/// A focus read's answer. Only the performer creates one; the wire cannot.
#[cfg_attr(feature = "testing", derive(PartialEq, Eq))]
#[derive(Debug)]
pub struct FocusRead {
    pub pid: Pid,
    pub generation: RidingGeneration,
    pub window: Option<WindowId>,
}
impl EventTrigger for FocusLanded {
    type Event = FocusRead;
    fn is_matching(&self, _ev: &FocusRead) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Tabbed;

/// Front tab URL, from the extension. No app-activation event carries it.
#[cfg_attr(feature = "testing", derive(PartialEq, Eq))]
#[derive(Debug)]
pub struct TabEvent {
    pub url: String,
}
impl EventTrigger for Tabbed {
    type Event = TabEvent;
    fn is_matching(&self, _ev: &TabEvent) -> bool {
        true
    }
}

/// Quit from the menu bar or SIGTERM. Trigger and event are the same type.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Quit;

bind::self_trigger!(Quit);

/// Apps mercury has bindings for. `Other` is anything else.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum App {
    Chrome,
    Codex,
    Discord,
    Finder,
    Ghostty,
    Obsidian,
    Zed,
    #[default]
    Other,
}

impl App {
    /// Bundle id as `freddie_app_nav` reports it. Display names differ by who is asked.
    #[must_use]
    pub fn from_bundle_id(bundle_id: &str) -> Self {
        match bundle_id {
            "com.google.Chrome" => Self::Chrome,
            "com.openai.codex" => Self::Codex,
            "com.hnc.Discord" => Self::Discord,
            "com.apple.finder" => Self::Finder,
            "com.mitchellh.ghostty" => Self::Ghostty,
            "md.obsidian" => Self::Obsidian,
            "dev.zed.Zed" => Self::Zed,
            _ => Self::Other,
        }
    }

    /// Bundle id for `freddie_app_nav::foreground`. [`App::Other`] has none.
    #[must_use]
    pub const fn bundle_id(self) -> Option<&'static str> {
        match self {
            Self::Chrome => Some("com.google.Chrome"),
            Self::Codex => Some("com.openai.codex"),
            Self::Discord => Some("com.hnc.Discord"),
            Self::Finder => Some("com.apple.finder"),
            Self::Ghostty => Some("com.mitchellh.ghostty"),
            Self::Obsidian => Some("md.obsidian"),
            Self::Zed => Some("dev.zed.Zed"),
            Self::Other => None,
        }
    }
}

/// The site a tab belongs to. `Other` has no bindings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Site {
    ClaudeAi,
    Other,
}

impl Site {
    /// Exact host match: `claude.ai.evil.com` is [`Site::Other`].
    #[must_use]
    pub fn from_url(url: &str) -> Self {
        match site_host(url) {
            Some("claude.ai") => Self::ClaudeAi,
            _ => Self::Other,
        }
    }
}

/// Host without a leading `www.`. `www.claude.ai` and `claude.ai` are the same site.
fn site_host(url: &str) -> Option<&str> {
    host(url).map(|host| host.strip_prefix("www.").unwrap_or(host))
}

/// Host as it appears, without port or userinfo. `None` for `about:blank` and `file:///...`.
///
/// Keeps `www.` because that is what a copy puts on the clipboard. Chrome already lowercases the host.
#[must_use]
pub fn host(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://")?.1;
    let authority = after_scheme
        .find(['/', '?', '#'])
        .map_or(after_scheme, |end| &after_scheme[..end]);
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host_port
        .find(':')
        .map_or(host_port, |end| &host_port[..end]);
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::{Site, host, site_host};

    #[test]
    fn the_host_is_the_one_the_url_carries() {
        for (url, want) in [
            ("https://claude.ai/new", Some("claude.ai")),
            ("https://claude.ai", Some("claude.ai")),
            ("https://claude.ai?q=1", Some("claude.ai")),
            ("https://claude.ai#top", Some("claude.ai")),
            ("https://www.x.com/asdfasdf", Some("www.x.com")),
            ("http://claude.ai:8080/x", Some("claude.ai")),
            ("https://user:pw@claude.ai/x", Some("claude.ai")),
            ("https://claude.ai.evil.com/", Some("claude.ai.evil.com")),
            ("https://notclaude.ai/", Some("notclaude.ai")),
            ("chrome://extensions", Some("extensions")),
            ("about:blank", None),
            ("file:///Users/x", None),
            ("", None),
        ] {
            assert_eq!(host(url), want, "{url}");
        }
    }

    #[test]
    fn the_site_host_drops_the_www() {
        for (url, want) in [
            ("https://www.claude.ai/x", Some("claude.ai")),
            ("https://claude.ai/x", Some("claude.ai")),
            ("https://www.x.com/asdfasdf", Some("x.com")),
            ("https://notwww.claude.ai/", Some("notwww.claude.ai")),
            ("about:blank", None),
        ] {
            assert_eq!(site_host(url), want, "{url}");
        }
    }

    #[test]
    fn only_the_exact_host_is_the_site() {
        for (url, want) in [
            ("https://claude.ai/new", Site::ClaudeAi),
            ("https://www.claude.ai/", Site::ClaudeAi),
            ("https://claude.ai.evil.com/", Site::Other),
            ("https://evil.com/claude.ai", Site::Other),
            ("about:blank", Site::Other),
        ] {
            assert_eq!(Site::from_url(url), want, "{url}");
        }
    }
}
