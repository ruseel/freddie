//! Platform-neutral keyboard vocabulary: [`Key`], [`KeyEvent`], modifier flags, and bind triggers.

use bind::EventTrigger;

/// A physical key, named by its US-ANSI position, independent of layout or OS.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    Escape,
    Return,
    Space,
    Tab,
    Backspace,
    Delete,
    CapsLock,

    UpArrow,
    DownArrow,
    LeftArrow,
    RightArrow,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,

    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    MetaLeft,
    MetaRight,

    Grave,
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    BackSlash,
    SemiColon,
    Quote,
    Comma,
    Dot,
    Slash,

    /// A native key code with no name. Not portable across OSes.
    Raw(u16),
}

/// Whether a key went down or came up.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PressType {
    Down,
    Up,
}

/// A key going down or coming up, carrying its modifier flags.
///
/// The flags are authoritative: the source stamps them at creation. A passed-through key
/// carries exactly these; a sync sweep or a chord builds its own.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub press: PressType,
    pub flags: ModifierFlags,
}

/// Portable modifier bitset. Stated on the event rather than read from a `CGEvent` source,
/// which lags a modifier posted microseconds earlier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierFlags(u8);

impl std::fmt::Debug for KeyEvent {
    /// `KeyEvent { key: KeyJ, press: Down }`, with `flags` only when some modifier is set.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KeyEvent {{ key: {:?}, press: {:?}",
            self.key, self.press
        )?;
        if !self.flags.is_empty() {
            write!(f, ", flags: {:?}", self.flags)?;
        }
        f.write_str(" }")
    }
}

impl std::fmt::Debug for ModifierFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ModifierFlags(")?;
        let mut any = false;
        for (name, flag) in [
            ("CONTROL", Self::CONTROL),
            ("COMMAND", Self::COMMAND),
            ("ALT", Self::ALT),
            ("SHIFT", Self::SHIFT),
            ("FN", Self::FN),
        ] {
            if self.contains(flag) {
                if any {
                    f.write_str("|")?;
                }
                f.write_str(name)?;
                any = true;
            }
        }
        f.write_str(")")
    }
}

impl std::ops::BitOr for ModifierFlags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl ModifierFlags {
    pub const CONTROL: Self = Self(1 << 0);
    pub const COMMAND: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    pub const SHIFT: Self = Self(1 << 3);
    /// The `fn` (Globe) modifier. Arrives only as a flag on other events, not as a key.
    pub const FN: Self = Self(1 << 4);

    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }

    pub const fn set(&mut self, flag: Self, on: bool) {
        self.0 = if on {
            self.0 | flag.0
        } else {
            self.0 & !flag.0
        };
    }
}

impl EventTrigger for Key {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        *self == event.key
    }
}

impl Key {
    #[must_use]
    pub const fn down(self) -> KeyPress {
        KeyPress {
            key: self,
            press: PressType::Down,
        }
    }

    #[must_use]
    pub const fn up(self) -> KeyPress {
        KeyPress {
            key: self,
            press: PressType::Up,
        }
    }

    /// Trigger matching this key going either direction.
    #[must_use]
    pub const fn press(self) -> KeyEitherPress {
        KeyEitherPress { key: self }
    }

    /// Control, command, alt, or shift, left or right. Caps lock and fn are not.
    #[must_use]
    pub const fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::ControlLeft
                | Self::ControlRight
                | Self::MetaLeft
                | Self::MetaRight
                | Self::AltLeft
                | Self::AltRight
                | Self::ShiftLeft
                | Self::ShiftRight
        )
    }

    #[must_use]
    pub const fn is_letter(self) -> bool {
        matches!(
            self,
            Self::KeyA
                | Self::KeyB
                | Self::KeyC
                | Self::KeyD
                | Self::KeyE
                | Self::KeyF
                | Self::KeyG
                | Self::KeyH
                | Self::KeyI
                | Self::KeyJ
                | Self::KeyK
                | Self::KeyL
                | Self::KeyM
                | Self::KeyN
                | Self::KeyO
                | Self::KeyP
                | Self::KeyQ
                | Self::KeyR
                | Self::KeyS
                | Self::KeyT
                | Self::KeyU
                | Self::KeyV
                | Self::KeyW
                | Self::KeyX
                | Self::KeyY
                | Self::KeyZ
        )
    }

    #[must_use]
    pub const fn is_number_row(self) -> bool {
        matches!(
            self,
            Self::Num0
                | Self::Num1
                | Self::Num2
                | Self::Num3
                | Self::Num4
                | Self::Num5
                | Self::Num6
                | Self::Num7
                | Self::Num8
                | Self::Num9
        )
    }

    #[must_use]
    pub const fn is_function(self) -> bool {
        matches!(
            self,
            Self::F1
                | Self::F2
                | Self::F3
                | Self::F4
                | Self::F5
                | Self::F6
                | Self::F7
                | Self::F8
                | Self::F9
                | Self::F10
                | Self::F11
                | Self::F12
                | Self::F13
                | Self::F14
                | Self::F15
                | Self::F16
                | Self::F17
                | Self::F18
                | Self::F19
                | Self::F20
                | Self::F21
                | Self::F22
                | Self::F23
                | Self::F24
        )
    }

    #[must_use]
    pub const fn is_arrow(self) -> bool {
        matches!(
            self,
            Self::UpArrow | Self::DownArrow | Self::LeftArrow | Self::RightArrow
        )
    }

    #[must_use]
    pub const fn is_navigation(self) -> bool {
        matches!(self, Self::Home | Self::End | Self::PageUp | Self::PageDown)
    }
}

/// A set of physical keys treated as one bind target.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KeyGroup {
    Any,
    Letter,
    Number,
    Function,
    Modifier,
    Arrow,
    Navigation,
}

impl KeyGroup {
    #[must_use]
    pub const fn contains(self, key: Key) -> bool {
        match self {
            Self::Any => true,
            Self::Letter => key.is_letter(),
            Self::Number => key.is_number_row(),
            Self::Function => key.is_function(),
            Self::Modifier => key.is_modifier(),
            Self::Arrow => key.is_arrow(),
            Self::Navigation => key.is_navigation(),
        }
    }

    #[must_use]
    pub const fn down(self) -> KeyGroupPress {
        KeyGroupPress {
            group: self,
            press: PressType::Down,
        }
    }

    #[must_use]
    pub const fn up(self) -> KeyGroupPress {
        KeyGroupPress {
            group: self,
            press: PressType::Up,
        }
    }

    #[must_use]
    pub const fn press(self) -> KeyGroupEitherPress {
        KeyGroupEitherPress { group: self }
    }
}

impl EventTrigger for KeyGroup {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.contains(event.key)
    }
}

/// A key from a [`KeyGroup`] going one direction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyGroupPress {
    pub group: KeyGroup,
    pub press: PressType,
}

impl EventTrigger for KeyGroupPress {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.group.contains(event.key) && event.press == self.press
    }
}

impl KeyGroupPress {
    #[must_use]
    pub const fn with(self, flags: ModifierFlags) -> KeyGroupChord {
        KeyGroupChord {
            group: self.group,
            press: self.press,
            flags,
        }
    }

    #[must_use]
    pub const fn bare(self) -> KeyGroupChord {
        self.with(ModifierFlags::empty())
    }
}

/// A key from a [`KeyGroup`] going one direction with exactly these modifiers held.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyGroupChord {
    pub group: KeyGroup,
    pub press: PressType,
    pub flags: ModifierFlags,
}

impl EventTrigger for KeyGroupChord {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.group.contains(event.key) && event.press == self.press && event.flags == self.flags
    }
}

/// A key going one direction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyPress {
    pub key: Key,
    pub press: PressType,
}

impl EventTrigger for KeyPress {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.key == event.key && self.press == event.press
    }
}

impl KeyPress {
    #[must_use]
    pub const fn with(self, flags: ModifierFlags) -> KeyChord {
        KeyChord {
            key: self.key,
            press: self.press,
            flags,
        }
    }

    /// Match only when no modifier is held. A node that binds one key at several
    /// modifier combinations writes every one of them as a chord.
    #[must_use]
    pub const fn bare(self) -> KeyChord {
        self.with(ModifierFlags::empty())
    }
}

/// A key going one direction with exactly these modifiers held.
/// Caps lock is not a [`ModifierFlags`] bit, so a chord matches with caps lock on or off.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyChord {
    pub key: Key,
    pub press: PressType,
    pub flags: ModifierFlags,
}

impl EventTrigger for KeyChord {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.key == event.key && self.press == event.press && self.flags == event.flags
    }
}

/// A key going either direction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyEitherPress {
    pub key: Key,
}

impl EventTrigger for KeyEitherPress {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.key == event.key
    }
}

impl KeyEitherPress {
    #[must_use]
    pub const fn with(self, flags: ModifierFlags) -> KeyEitherChord {
        KeyEitherChord {
            key: self.key,
            flags,
        }
    }

    #[must_use]
    pub const fn bare(self) -> KeyEitherChord {
        self.with(ModifierFlags::empty())
    }
}

/// A key going either direction with exactly these modifiers held.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyEitherChord {
    pub key: Key,
    pub flags: ModifierFlags,
}

impl EventTrigger for KeyEitherChord {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.key == event.key && event.flags == self.flags
    }
}

/// A group's keys going either direction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyGroupEitherPress {
    pub group: KeyGroup,
}

impl EventTrigger for KeyGroupEitherPress {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.group.contains(event.key)
    }
}

impl KeyGroupEitherPress {
    #[must_use]
    pub const fn with(self, flags: ModifierFlags) -> KeyGroupEitherChord {
        KeyGroupEitherChord {
            group: self.group,
            flags,
        }
    }

    #[must_use]
    pub const fn bare(self) -> KeyGroupEitherChord {
        self.with(ModifierFlags::empty())
    }
}

/// A group's keys going either direction with exactly these modifiers held.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyGroupEitherChord {
    pub group: KeyGroup,
    pub flags: ModifierFlags,
}

impl EventTrigger for KeyGroupEitherChord {
    type Event = KeyEvent;

    fn is_matching(&self, event: &KeyEvent) -> bool {
        self.group.contains(event.key) && event.flags == self.flags
    }
}

/// A key event tagged with the consumer's device identity `D`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeviceKeyed<E, D> {
    pub key: E,
    pub device: D,
}

/// Restricts an inner key trigger to devices the filter `D` matches.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct OnDevice<T, D> {
    pub device: D,
    pub inner: T,
}

impl<T, D> EventTrigger for OnDevice<T, D>
where
    T: EventTrigger,
    D: EventTrigger,
{
    type Event = DeviceKeyed<T::Event, D::Event>;

    fn is_matching(&self, event: &Self::Event) -> bool {
        self.device.is_matching(&event.device) && self.inner.is_matching(&event.key)
    }
}

impl<T, D> OnDevice<T, D> {
    #[must_use]
    pub const fn new(device: D, inner: T) -> Self {
        Self { device, inner }
    }
}

pub trait WithDevice: Sized {
    #[must_use]
    fn on_device<D: EventTrigger>(self, device: D) -> OnDevice<Self, D> {
        OnDevice {
            device,
            inner: self,
        }
    }
}

impl<T: EventTrigger> WithDevice for T {}

#[cfg(test)]
mod tests {
    use super::{Key, KeyEvent, KeyGroup, ModifierFlags, PressType};
    use bind::EventTrigger;

    #[test]
    fn matches_only_its_own_key() {
        let event = KeyEvent {
            key: Key::KeyR,
            press: PressType::Down,
            flags: ModifierFlags::empty(),
        };
        assert!(Key::KeyR.is_matching(&event));
        assert!(!Key::KeyS.is_matching(&event));
    }

    #[test]
    fn key_group_categories_partition_keys() {
        assert!(KeyGroup::Letter.contains(Key::KeyA));
        assert!(!KeyGroup::Letter.contains(Key::Num1));
        assert!(KeyGroup::Number.contains(Key::Num0));
        assert!(!KeyGroup::Number.contains(Key::KeyA));
        assert!(KeyGroup::Function.contains(Key::F19));
        assert!(!KeyGroup::Function.contains(Key::Escape));
        assert!(KeyGroup::Modifier.contains(Key::ShiftLeft));
        assert!(!KeyGroup::Modifier.contains(Key::CapsLock));
        assert!(KeyGroup::Arrow.contains(Key::LeftArrow));
        assert!(KeyGroup::Navigation.contains(Key::Home));
        assert!(!KeyGroup::Navigation.contains(Key::Insert));
        assert!(KeyGroup::Any.contains(Key::Raw(0)));
    }

    #[test]
    fn key_group_number_matches_the_number_row_only() {
        let one = KeyEvent {
            key: Key::Num1,
            press: PressType::Down,
            flags: ModifierFlags::empty(),
        };
        let a = KeyEvent {
            key: Key::KeyA,
            press: PressType::Down,
            flags: ModifierFlags::empty(),
        };
        assert!(KeyGroup::Number.is_matching(&one));
        assert!(!KeyGroup::Number.is_matching(&a));
        assert!(KeyGroup::Any.is_matching(&a));
        assert!(KeyGroup::Number.down().bare().is_matching(&one));
        assert!(
            !KeyGroup::Number
                .down()
                .with(ModifierFlags::SHIFT)
                .is_matching(&one)
        );
        let shifted = KeyEvent {
            key: Key::Num5,
            press: PressType::Down,
            flags: ModifierFlags::SHIFT,
        };
        assert!(
            KeyGroup::Number
                .down()
                .with(ModifierFlags::SHIFT)
                .is_matching(&shifted)
        );
    }

    #[test]
    fn debug_leaves_out_flags_when_there_are_none() {
        let bare = KeyEvent {
            key: Key::KeyJ,
            press: PressType::Down,
            flags: ModifierFlags::empty(),
        };
        assert_eq!(format!("{bare:?}"), "KeyEvent { key: KeyJ, press: Down }");

        let mut flags = ModifierFlags::COMMAND;
        flags.set(ModifierFlags::SHIFT, true);
        let chord = KeyEvent {
            key: Key::KeyV,
            press: PressType::Up,
            flags,
        };
        assert_eq!(
            format!("{chord:?}"),
            "KeyEvent { key: KeyV, press: Up, flags: ModifierFlags(COMMAND|SHIFT) }"
        );
    }

    #[test]
    fn a_chord_matches_only_its_own_modifiers() {
        let bare = KeyEvent {
            key: Key::KeyL,
            press: PressType::Down,
            flags: ModifierFlags::empty(),
        };
        let with_command = KeyEvent {
            key: Key::KeyL,
            press: PressType::Down,
            flags: ModifierFlags::COMMAND,
        };
        let with_both = KeyEvent {
            key: Key::KeyL,
            press: PressType::Down,
            flags: ModifierFlags::COMMAND | ModifierFlags::SHIFT,
        };

        for (trigger, matching) in [
            (Key::KeyL.down().bare(), &bare),
            (Key::KeyL.down().with(ModifierFlags::COMMAND), &with_command),
            (
                Key::KeyL
                    .down()
                    .with(ModifierFlags::COMMAND | ModifierFlags::SHIFT),
                &with_both,
            ),
        ] {
            for event in [&bare, &with_command, &with_both] {
                assert_eq!(
                    trigger.is_matching(event),
                    std::ptr::eq(event, matching),
                    "{trigger:?} against {event:?}"
                );
            }
        }
    }

    #[test]
    fn a_chord_matches_neither_the_other_key_nor_the_release() {
        let trigger = Key::KeyL.down().with(ModifierFlags::COMMAND);
        assert!(!trigger.is_matching(&KeyEvent {
            key: Key::KeyK,
            press: PressType::Down,
            flags: ModifierFlags::COMMAND,
        }));
        assert!(!trigger.is_matching(&KeyEvent {
            key: Key::KeyL,
            press: PressType::Up,
            flags: ModifierFlags::COMMAND,
        }));
    }

    #[test]
    fn a_press_matches_whatever_modifiers_are_held() {
        let trigger = Key::KeyL.down();
        assert!(trigger.is_matching(&KeyEvent {
            key: Key::KeyL,
            press: PressType::Down,
            flags: ModifierFlags::COMMAND,
        }));
    }

    #[test]
    fn raw_matches_by_code() {
        let event = KeyEvent {
            key: Key::Raw(64000),
            press: PressType::Down,
            flags: ModifierFlags::empty(),
        };
        assert!(Key::Raw(64000).is_matching(&event));
        assert!(!Key::Raw(1).is_matching(&event));
        assert!(!Key::KeyA.is_matching(&event));
    }

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    enum TestDevice {
        A,
        B,
    }

    bind::self_trigger!(TestDevice);

    #[test]
    fn on_device_matches_key_and_device() {
        use super::{DeviceKeyed, WithDevice};

        let trigger = Key::KeyR.down().on_device(TestDevice::A);
        let matching = DeviceKeyed {
            key: KeyEvent {
                key: Key::KeyR,
                press: PressType::Down,
                flags: ModifierFlags::empty(),
            },
            device: TestDevice::A,
        };
        assert!(trigger.is_matching(&matching));
        assert!(!trigger.is_matching(&DeviceKeyed {
            key: KeyEvent {
                key: Key::KeyR,
                press: PressType::Down,
                flags: ModifierFlags::empty(),
            },
            device: TestDevice::B,
        }));
        assert!(!trigger.is_matching(&DeviceKeyed {
            key: KeyEvent {
                key: Key::KeyS,
                press: PressType::Down,
                flags: ModifierFlags::empty(),
            },
            device: TestDevice::A,
        }));
    }
}
