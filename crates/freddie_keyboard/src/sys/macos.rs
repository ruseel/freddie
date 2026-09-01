//! The macOS backend, on `core-graphics`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use core_foundation::runloop::CFRunLoop;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CGKeyCode, CallbackResult, EventField, KeyCode,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use freddie_hid_device::{DeviceInfo, ResolveFailure, SourceId, resolve, source_of};
use freddie_keys::{Key, KeyEvent, ModifierFlags, PressType};
use freddie_keys::{MouseButton, MouseButtonEvent};
use objc2::rc::autoreleasepool;

use crate::{CaptureError, EmitError};

// ---------------------------------------------------------------------------
// Pure logic.
// ---------------------------------------------------------------------------

// Every named key and its macOS virtual key code. Keys with no macOS code
// (F21-F24, Insert) are absent, so `to_code` gives `None` and `from_code` gives
// `Key::Raw`.
const TABLE: &[(Key, CGKeyCode)] = &[
    (Key::KeyA, KeyCode::ANSI_A),
    (Key::KeyB, KeyCode::ANSI_B),
    (Key::KeyC, KeyCode::ANSI_C),
    (Key::KeyD, KeyCode::ANSI_D),
    (Key::KeyE, KeyCode::ANSI_E),
    (Key::KeyF, KeyCode::ANSI_F),
    (Key::KeyG, KeyCode::ANSI_G),
    (Key::KeyH, KeyCode::ANSI_H),
    (Key::KeyI, KeyCode::ANSI_I),
    (Key::KeyJ, KeyCode::ANSI_J),
    (Key::KeyK, KeyCode::ANSI_K),
    (Key::KeyL, KeyCode::ANSI_L),
    (Key::KeyM, KeyCode::ANSI_M),
    (Key::KeyN, KeyCode::ANSI_N),
    (Key::KeyO, KeyCode::ANSI_O),
    (Key::KeyP, KeyCode::ANSI_P),
    (Key::KeyQ, KeyCode::ANSI_Q),
    (Key::KeyR, KeyCode::ANSI_R),
    (Key::KeyS, KeyCode::ANSI_S),
    (Key::KeyT, KeyCode::ANSI_T),
    (Key::KeyU, KeyCode::ANSI_U),
    (Key::KeyV, KeyCode::ANSI_V),
    (Key::KeyW, KeyCode::ANSI_W),
    (Key::KeyX, KeyCode::ANSI_X),
    (Key::KeyY, KeyCode::ANSI_Y),
    (Key::KeyZ, KeyCode::ANSI_Z),
    (Key::Num0, KeyCode::ANSI_0),
    (Key::Num1, KeyCode::ANSI_1),
    (Key::Num2, KeyCode::ANSI_2),
    (Key::Num3, KeyCode::ANSI_3),
    (Key::Num4, KeyCode::ANSI_4),
    (Key::Num5, KeyCode::ANSI_5),
    (Key::Num6, KeyCode::ANSI_6),
    (Key::Num7, KeyCode::ANSI_7),
    (Key::Num8, KeyCode::ANSI_8),
    (Key::Num9, KeyCode::ANSI_9),
    (Key::F1, KeyCode::F1),
    (Key::F2, KeyCode::F2),
    (Key::F3, KeyCode::F3),
    (Key::F4, KeyCode::F4),
    (Key::F5, KeyCode::F5),
    (Key::F6, KeyCode::F6),
    (Key::F7, KeyCode::F7),
    (Key::F8, KeyCode::F8),
    (Key::F9, KeyCode::F9),
    (Key::F10, KeyCode::F10),
    (Key::F11, KeyCode::F11),
    (Key::F12, KeyCode::F12),
    (Key::F13, KeyCode::F13),
    (Key::F14, KeyCode::F14),
    (Key::F15, KeyCode::F15),
    (Key::F16, KeyCode::F16),
    (Key::F17, KeyCode::F17),
    (Key::F18, KeyCode::F18),
    (Key::F19, KeyCode::F19),
    (Key::F20, KeyCode::F20),
    (Key::Escape, KeyCode::ESCAPE),
    (Key::Return, KeyCode::RETURN),
    (Key::Space, KeyCode::SPACE),
    (Key::Tab, KeyCode::TAB),
    (Key::Backspace, KeyCode::DELETE),
    (Key::Delete, KeyCode::FORWARD_DELETE),
    (Key::CapsLock, KeyCode::CAPS_LOCK),
    (Key::UpArrow, KeyCode::UP_ARROW),
    (Key::DownArrow, KeyCode::DOWN_ARROW),
    (Key::LeftArrow, KeyCode::LEFT_ARROW),
    (Key::RightArrow, KeyCode::RIGHT_ARROW),
    (Key::Home, KeyCode::HOME),
    (Key::End, KeyCode::END),
    (Key::PageUp, KeyCode::PAGE_UP),
    (Key::PageDown, KeyCode::PAGE_DOWN),
    (Key::ShiftLeft, KeyCode::SHIFT),
    (Key::ShiftRight, KeyCode::RIGHT_SHIFT),
    (Key::ControlLeft, KeyCode::CONTROL),
    (Key::ControlRight, KeyCode::RIGHT_CONTROL),
    (Key::AltLeft, KeyCode::OPTION),
    (Key::AltRight, KeyCode::RIGHT_OPTION),
    (Key::MetaLeft, KeyCode::COMMAND),
    (Key::MetaRight, KeyCode::RIGHT_COMMAND),
    (Key::Grave, KeyCode::ANSI_GRAVE),
    (Key::Minus, KeyCode::ANSI_MINUS),
    (Key::Equal, KeyCode::ANSI_EQUAL),
    (Key::LeftBracket, KeyCode::ANSI_LEFT_BRACKET),
    (Key::RightBracket, KeyCode::ANSI_RIGHT_BRACKET),
    (Key::BackSlash, KeyCode::ANSI_BACKSLASH),
    (Key::SemiColon, KeyCode::ANSI_SEMICOLON),
    (Key::Quote, KeyCode::ANSI_QUOTE),
    (Key::Comma, KeyCode::ANSI_COMMA),
    (Key::Dot, KeyCode::ANSI_PERIOD),
    (Key::Slash, KeyCode::ANSI_SLASH),
];

fn to_code(key: Key) -> Option<CGKeyCode> {
    if let Key::Raw(code) = key {
        return Some(code);
    }
    TABLE.iter().find(|(k, _)| *k == key).map(|(_, code)| *code)
}

fn from_code(code: CGKeyCode) -> Key {
    TABLE
        .iter()
        .find(|(_, c)| *c == code)
        .map_or(Key::Raw(code), |(key, _)| *key)
}

#[derive(PartialEq, Eq, Debug)]
enum Decision {
    Pass,
    Remap(KeyEvent),
    Drop,
}

fn decide(input: &KeyEvent, out: Option<KeyEvent>) -> Decision {
    match out {
        None => Decision::Drop,
        Some(ref e) if e == input => Decision::Pass,
        Some(e) => Decision::Remap(e),
    }
}

// ---------------------------------------------------------------------------
// The tap and the posting (FFI).
// ---------------------------------------------------------------------------

/// Marker an emitted event carries so the interceptor skips its own output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tag(i64);

impl Tag {
    /// Per-process random, so an interceptor skips only its own output.
    fn new() -> Self {
        let mut h = RandomState::new().build_hasher();
        h.write_u8(0);
        Self(i64::from_ne_bytes(h.finish().to_ne_bytes()))
    }

    fn stamp(self, event: &CGEvent) {
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, self.0);
    }

    fn marks(self, event: &CGEvent) -> bool {
        event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == self.0
    }
}

fn keycode(event: &CGEvent) -> Option<CGKeyCode> {
    u16::try_from(event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)).ok()
}

const MOUSE_BUTTON_BACK: i64 = 3;
const MOUSE_BUTTON_FORWARD: i64 = 4;

fn mouse_button_number(event: &CGEvent) -> i64 {
    event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER)
}

/// Physical down of each `FlagsChanged` key. Direction is a toggle of this set, not the
/// event's flag bit: that bit is shared by both sides of a modifier, and `CapsLock`'s
/// `AlphaShift` is a latch.
struct FlagsChangedDown {
    bits: Cell<u16>,
}

impl FlagsChangedDown {
    const fn new() -> Self {
        Self { bits: Cell::new(0) }
    }

    fn toggle(&self, key: Key) -> Option<PressType> {
        let bit = flags_changed_bit(key)?;
        let held = self.bits.get();
        if held & bit != 0 {
            self.bits.set(held & !bit);
            Some(PressType::Up)
        } else {
            self.bits.set(held | bit);
            Some(PressType::Down)
        }
    }
}

const fn flags_changed_bit(key: Key) -> Option<u16> {
    Some(match key {
        Key::ShiftLeft => 0x0001,
        Key::ShiftRight => 0x0002,
        Key::ControlLeft => 0x0004,
        Key::ControlRight => 0x0008,
        Key::AltLeft => 0x0010,
        Key::AltRight => 0x0020,
        Key::MetaLeft => 0x0040,
        Key::MetaRight => 0x0080,
        Key::CapsLock => 0x0100,
        _ => return None,
    })
}

fn press_of_key(kind: CGEventType, key: Key, down: &FlagsChangedDown) -> Option<PressType> {
    match kind {
        CGEventType::KeyDown => Some(PressType::Down),
        CGEventType::KeyUp => Some(PressType::Up),
        CGEventType::FlagsChanged => down.toggle(key),
        _ => None,
    }
}

/// A keyboard event for `key`, carrying exactly `flags`, from a long-lived private source.
///
/// Each `CGEventSourceCreate(Private)` maps about 16KB that `CFRelease` never unmaps, so
/// a source per event grows the process by 16KB per keystroke. The flags on the wire are
/// `to_cg(flags) | intrinsic_flags(code)`: posting through a source mutates it (an arrow
/// leaves `NumericPad` in the source), and reading birth flags back would put that bit on
/// a later `cmd`-`space`. Not a `NULL` source, which inherits the shared session state.
///
/// # Errors
///
/// [`EmitError::Unmappable`] if the key has no code, [`EmitError::Post`] if the OS refused.
fn keyboard_event(
    source: &CGEventSource,
    key: Key,
    press: PressType,
    flags: ModifierFlags,
) -> Result<CGEvent, EmitError> {
    let code = to_code(key).ok_or(EmitError::Unmappable(key))?;
    // `new_keyboard_event` takes the source by value; the clone is a `CFRetain` of the same
    // source, not a second mapping.
    let event = CGEvent::new_keyboard_event(source.clone(), code, press == PressType::Down)
        .map_err(|_| EmitError::Post)?;
    let intrinsic = intrinsic_flags(code);
    event.set_flags(to_cg(flags) | intrinsic);
    // Raw flag bits and OS event type. Two presses that dispatch identically can still post
    // differently.
    tracing::debug!(
        ?key,
        ?press,
        raw_flags = %format!("{:#010x}", event.get_flags().bits()),
        intrinsic = %format!("{:#010x}", intrinsic.bits()),
        kind = ?event.get_type(),
        "post"
    );
    Ok(event)
}

/// Grab the keyboard. The interceptor decides via `on_key`; the emitter synthesizes
/// keys, tagged so the interceptor passes them.
///
/// # Errors
///
/// [`CaptureError`] if the tap cannot be installed (usually missing Accessibility).
pub fn intercept(
    on_key: impl Fn(KeyEvent) -> Option<KeyEvent> + Send + 'static,
) -> Result<(Interceptor, Emitter), CaptureError> {
    run_tap(move |input, _event| on_key(input))
}

/// Same tap as [`intercept`], with per-key source device categorization.
/// `categorize` runs once per distinct HID source.
///
/// # Errors
///
/// [`CaptureError`] if the tap cannot be installed.
pub fn intercept_with_source<T, C, F>(
    mut categorize: C,
    on_key: F,
) -> Result<(Interceptor, Emitter), CaptureError>
where
    T: Clone + Send + 'static,
    C: FnMut(Option<Result<DeviceInfo, (SourceId, ResolveFailure)>>) -> T + Send + 'static,
    F: Fn((KeyEvent, T)) -> Option<KeyEvent> + Send + 'static,
{
    let mut by_source: HashMap<SourceId, T> = HashMap::new();
    run_tap(move |input, event| {
        let class = match source_of(event) {
            None => categorize(None),
            Some(id) => by_source
                .entry(id)
                .or_insert_with(|| {
                    let resolved = resolve(id);
                    tracing::debug!(source_id = id.0, resolved = ?resolved, "new key source");
                    categorize(Some(resolved))
                })
                .clone(),
        };
        on_key((input, class))
    })
}

/// Shared tap install. Only [`intercept_with_source`] reads the live `CGEvent`.
fn run_tap(
    on_key: impl FnMut(KeyEvent, &CGEvent) -> Option<KeyEvent> + Send + 'static,
) -> Result<(Interceptor, Emitter), CaptureError> {
    let tag = Tag::new();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<CFRunLoop, ()>>();
    let signal = ready_tx.clone();
    // The tap callback is `Fn`, not `FnMut`; cache-owning categorize needs mutability.
    let on_key = RefCell::new(on_key);

    let thread = std::thread::spawn(move || {
        // One remap source on this thread. A source per remapped key would map 16KB each.
        let Ok(remap_source) = CGEventSource::new(CGEventSourceStateID::Private) else {
            let _ = signal.send(Err(()));
            return;
        };
        let flags_changed_down = FlagsChangedDown::new();
        let outcome = CGEventTap::with_enabled(
            CGEventTapLocation::Session,
            // Head so Drop can stop CapsLock before the OS latches AlphaShift.
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, kind, event| {
                if tag.marks(event) {
                    return CallbackResult::Keep; // our own emit
                }
                let Some(code) = keycode(event) else {
                    return CallbackResult::Keep;
                };
                let key = from_code(code);
                let Some(press) = press_of_key(kind, key, &flags_changed_down) else {
                    return CallbackResult::Keep;
                };
                let input = KeyEvent {
                    key,
                    press,
                    // The modifiers the source baked onto this event. A modifier delivered as a
                    // flag rather than as its own key (an injected `cmd`-`v`, or `fn`) lives only
                    // here, so read it or it is lost.
                    flags: from_cg(event.get_flags()),
                };
                // Physical HID input is PID 0; a userspace `CGEventPost` (another app) is nonzero.
                // Logged only. Own emits are tagged and returned above.
                let source_pid =
                    event.get_integer_value_field(EventField::EVENT_SOURCE_UNIX_PROCESS_ID);
                tracing::debug!(?input, source_pid, "tap");
                match decide(&input, on_key.borrow_mut()(input.clone(), event)) {
                    Decision::Pass => CallbackResult::Keep,
                    Decision::Drop => CallbackResult::Drop,
                    Decision::Remap(out) => {
                        match keyboard_event(&remap_source, out.key, out.press, out.flags) {
                            Ok(event) => CallbackResult::Replace(event),
                            Err(e) => {
                                tracing::warn!(key = ?out.key, error = %e, "dropped a remapped key");
                                CallbackResult::Drop
                            }
                        }
                    }
                }
            },
            || {
                let _ = ready_tx.send(Ok(CFRunLoop::get_current()));
                CFRunLoop::run_current();
            },
        );
        if outcome.is_err() {
            let _ = signal.send(Err(()));
        }
    });

    let Ok(Ok(run_loop)) = ready_rx.recv() else {
        return Err(CaptureError);
    };
    let interceptor = Interceptor {
        _tap: TapThread {
            run_loop,
            thread: Some(thread),
        },
    };
    // The emitter's source on the posting thread. Failure is a `CaptureError`: both halves
    // or neither.
    let source = CGEventSource::new(CGEventSourceStateID::Private).map_err(|()| CaptureError)?;
    let emitter = Emitter { tag, source };
    Ok((interceptor, emitter))
}

/// An active grab of the mouse side buttons. Dropping it releases the grab.
pub struct MouseInterceptor {
    _tap: TapThread,
}

/// Intercept mouse side buttons (OtherMouse buttons 3 and 4).
/// The callback receives the button event; returning `None` swallows it.
///
/// # Errors
///
/// [`CaptureError`] if the tap cannot be installed.
pub fn intercept_mouse(
    tag: Tag,
    on_button: impl Fn(MouseButtonEvent) -> Option<MouseButtonEvent> + Send + 'static,
) -> Result<MouseInterceptor, CaptureError> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<CFRunLoop, ()>>();
    let signal = ready_tx.clone();

    let thread = std::thread::spawn(move || {
        let outcome = CGEventTap::with_enabled(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseUp,
            ],
            move |_proxy, kind, event| {
                if tag.marks(event) {
                    return CallbackResult::Keep;
                }
                let button_num = mouse_button_number(event);
                tracing::info!(?kind, button_num, "raw mouse event received in tap");
                let button = match button_num {
                    MOUSE_BUTTON_BACK => MouseButton::Back,
                    MOUSE_BUTTON_FORWARD => MouseButton::Forward,
                    _ => return CallbackResult::Keep,
                };
                let press = match kind {
                    CGEventType::OtherMouseDown => PressType::Down,
                    CGEventType::OtherMouseUp => PressType::Up,
                    _ => return CallbackResult::Keep,
                };
                let input = MouseButtonEvent { button, press };
                tracing::info!(?input, "intercepted mouse side button");
                match on_button(input) {
                    None => CallbackResult::Drop,
                    Some(_) => CallbackResult::Keep,
                }
            },
            || {
                tracing::info!("mouse tap run loop running");
                let _ = ready_tx.send(Ok(CFRunLoop::get_current()));
                CFRunLoop::run_current();
            },
        );
        if outcome.is_err() {
            tracing::error!("CGEventTap::with_enabled failed for mouse");
            let _ = signal.send(Err(()));
        }
    });

    let Ok(Ok(run_loop)) = ready_rx.recv() else {
        return Err(CaptureError);
    };
    Ok(MouseInterceptor {
        _tap: TapThread {
            run_loop,
            thread: Some(thread),
        },
    })
}

/// An active grab of the keyboard. Dropping it drops the [`TapThread`], which releases it.
pub struct Interceptor {
    _tap: TapThread,
}

/// How long a dropped [`TapThread`] waits for the tap thread. Waiting forever would turn
/// one wedged `on_key` into a process that cannot exit.
const RELEASE_TIMEOUT: Duration = Duration::from_millis(500);

/// The thread the event tap runs on. Stopping the run loop makes the thread return.
struct TapThread {
    run_loop: CFRunLoop,
    thread: Option<JoinHandle<()>>,
}

impl Drop for TapThread {
    fn drop(&mut self) {
        self.run_loop.stop();
        let Some(thread) = self.thread.take() else {
            return;
        };
        // Joined on another thread so this one can stop waiting. The tap is released when the
        // thread ends either way; what the timeout bounds is how long the caller waits to hear
        // about it.
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = thread.join();
            let _ = done_tx.send(());
        });
        if done_rx.recv_timeout(RELEASE_TIMEOUT).is_err() {
            tracing::warn!("the keyboard tap did not stop; releasing without it");
        }
    }
}

/// Keycodes a clean private source puts `NumericPad` on: the four arrows, and the keypad
/// apart from `ANSI_KEYPAD_CLEAR` and `JIS_KEYPAD_COMMA`. By keycode, not [`Key`]: the
/// keypad has no variant and arrives as `Key::Raw(code)`.
const NUMERIC_PAD_CODES: &[CGKeyCode] = &[
    KeyCode::LEFT_ARROW,
    KeyCode::RIGHT_ARROW,
    KeyCode::DOWN_ARROW,
    KeyCode::UP_ARROW,
    KeyCode::ANSI_KEYPAD_DECIMAL,
    KeyCode::ANSI_KEYPAD_MULTIPLY,
    KeyCode::ANSI_KEYPAD_PLUS,
    KeyCode::ANSI_KEYPAD_DIVIDE,
    KeyCode::ANSI_KEYPAD_ENTER,
    KeyCode::ANSI_KEYPAD_MINUS,
    KeyCode::ANSI_KEYPAD_EQUAL,
    KeyCode::ANSI_KEYPAD_0,
    KeyCode::ANSI_KEYPAD_1,
    KeyCode::ANSI_KEYPAD_2,
    KeyCode::ANSI_KEYPAD_3,
    KeyCode::ANSI_KEYPAD_4,
    KeyCode::ANSI_KEYPAD_5,
    KeyCode::ANSI_KEYPAD_6,
    KeyCode::ANSI_KEYPAD_7,
    KeyCode::ANSI_KEYPAD_8,
    KeyCode::ANSI_KEYPAD_9,
];

/// Non-modifier flag bits `code` carries of its own accord. `SecondaryFn` is a portable
/// [`ModifierFlags`] bit and arrives through `to_cg`.
fn intrinsic_flags(code: CGKeyCode) -> CGEventFlags {
    if NUMERIC_PAD_CODES.contains(&code) {
        CGEventFlags::CGEventFlagNumericPad
    } else {
        CGEventFlags::empty()
    }
}

const FLAG_PAIRS: [(ModifierFlags, CGEventFlags); 5] = [
    (ModifierFlags::CONTROL, CGEventFlags::CGEventFlagControl),
    (ModifierFlags::COMMAND, CGEventFlags::CGEventFlagCommand),
    (ModifierFlags::ALT, CGEventFlags::CGEventFlagAlternate),
    (ModifierFlags::SHIFT, CGEventFlags::CGEventFlagShift),
    (ModifierFlags::FN, CGEventFlags::CGEventFlagSecondaryFn),
];

fn to_cg(flags: ModifierFlags) -> CGEventFlags {
    let mut out = CGEventFlags::empty();
    for (portable, native) in FLAG_PAIRS {
        out.set(native, flags.contains(portable));
    }
    out
}

/// Portable flags an incoming event carries, so a passed-through key keeps a modifier that
/// was baked onto it (an injected `cmd`-`v`, or `fn`).
fn from_cg(flags: CGEventFlags) -> ModifierFlags {
    let mut out = ModifierFlags::empty();
    for (portable, native) in FLAG_PAIRS {
        out.set(portable, flags.contains(native));
    }
    out
}

/// Synthesizes keys through the interceptor's tag. `!Send`: a `CGEventSource` stays on
/// the thread that built it, and posting mutates it.
pub struct Emitter {
    tag: Tag,
    /// One source, created in [`run_tap`]. A source per event would map 16KB per keystroke.
    source: CGEventSource,
}

impl Emitter {
    #[must_use]
    pub const fn tag(&self) -> Tag {
        self.tag
    }

    /// Press then release `button`.
    ///
    /// # Errors
    ///
    /// [`EmitError`] if the event could not be posted.
    pub fn tap_mouse(&self, button: MouseButton) -> Result<(), EmitError> {
        let button_number: i64 = match button {
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
        };
        autoreleasepool(|_pool| {
            use core_graphics::geometry::CGPoint;
            let pos = match CGEvent::new(self.source.clone()) {
                Ok(ev) => ev.location(),
                Err(()) => CGPoint { x: 0.0, y: 0.0 },
            };
            for kind in [CGEventType::OtherMouseDown, CGEventType::OtherMouseUp] {
                let event = CGEvent::new_mouse_event(
                    self.source.clone(),
                    kind,
                    pos,
                    core_graphics::event::CGMouseButton::Center,
                )
                .map_err(|()| EmitError::Post)?;
                event.set_integer_value_field(
                    EventField::MOUSE_EVENT_BUTTON_NUMBER,
                    button_number,
                );
                self.tag.stamp(&event);
                event.post(CGEventTapLocation::Session);
            }
            Ok(())
        })
    }

    /// Post `key` going down or coming up, carrying exactly `flags`.
    /// `CGEventPost` autoreleases two `CFData`s per call; the daemon's worker has no pool
    /// of its own, so this drains one per post.
    fn post(&self, key: Key, press: PressType, flags: ModifierFlags) -> Result<(), EmitError> {
        autoreleasepool(|_pool| {
            let event = keyboard_event(&self.source, key, press, flags)?;
            self.tag.stamp(&event);
            event.post(CGEventTapLocation::Session);
            Ok(())
        })
    }

    /// # Errors
    ///
    /// [`EmitError`] if the key has no code or could not be posted.
    pub fn emit(&self, key: Key, press: PressType, flags: ModifierFlags) -> Result<(), EmitError> {
        self.post(key, press, flags)
    }

    /// Press then release `key`, both halves carrying `flags`. A chord bakes the modifier
    /// into the key's flags so no synthetic modifier event strands a real hold.
    ///
    /// # Errors
    ///
    /// [`EmitError`] if the key has no code or could not be posted.
    pub fn tap(&self, key: Key, flags: ModifierFlags) -> Result<(), EmitError> {
        self.emit(key, PressType::Down, flags)?;
        self.emit(key, PressType::Up, flags)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Decision, EmitError, FlagsChangedDown, Tag, decide, flags_changed_bit, from_code,
        intrinsic_flags, keyboard_event, press_of_key, to_cg, to_code,
    };
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, KeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use freddie_keys::{Key, KeyEvent, ModifierFlags, PressType};

    fn ev(key: Key) -> KeyEvent {
        KeyEvent {
            key,
            press: PressType::Down,
            flags: ModifierFlags::empty(),
        }
    }

    #[test]
    fn named_keys_round_trip() {
        assert_eq!(to_code(Key::KeyR), Some(KeyCode::ANSI_R));
        assert_eq!(from_code(KeyCode::ANSI_R), Key::KeyR);
        assert_eq!(to_code(Key::Escape), Some(KeyCode::ESCAPE));
        assert_eq!(from_code(KeyCode::ESCAPE), Key::Escape);
        assert_eq!(to_code(Key::MetaLeft), Some(KeyCode::COMMAND));
        assert_eq!(from_code(KeyCode::RIGHT_SHIFT), Key::ShiftRight);
    }

    #[test]
    fn unknown_code_becomes_raw() {
        assert_eq!(from_code(64000), Key::Raw(64000));
    }

    #[test]
    fn raw_round_trips_its_code() {
        assert_eq!(to_code(Key::Raw(64000)), Some(64000));
        assert_eq!(from_code(64000), Key::Raw(64000));
    }

    #[test]
    fn keys_without_a_mac_code_are_unmappable() {
        assert_eq!(to_code(Key::F24), None);
        assert_eq!(to_code(Key::Insert), None);
    }

    #[test]
    fn decide_passes_unchanged() {
        let a = ev(Key::KeyA);
        assert_eq!(decide(&a, Some(a.clone())), Decision::Pass);
    }

    #[test]
    fn decide_remaps_a_different_key() {
        let a = ev(Key::KeyA);
        let b = ev(Key::KeyB);
        assert_eq!(decide(&a, Some(b.clone())), Decision::Remap(b));
    }

    #[test]
    fn decide_drops_on_none() {
        assert_eq!(decide(&ev(Key::KeyA), None), Decision::Drop);
    }

    #[test]
    fn decide_remaps_when_only_press_changes() {
        let down = ev(Key::KeyA);
        let up = KeyEvent {
            key: Key::KeyA,
            press: PressType::Up,
            flags: ModifierFlags::empty(),
        };
        assert_eq!(decide(&down, Some(up.clone())), Decision::Remap(up));
    }

    // The six device-independent modifier bits. Tests name them; production names the bits
    // it emits.
    const MODIFIERS: CGEventFlags = CGEventFlags::from_bits_truncate(
        CGEventFlags::CGEventFlagAlphaShift.bits()
            | CGEventFlags::CGEventFlagShift.bits()
            | CGEventFlags::CGEventFlagControl.bits()
            | CGEventFlags::CGEventFlagAlternate.bits()
            | CGEventFlags::CGEventFlagCommand.bits()
            | CGEventFlags::CGEventFlagSecondaryFn.bits(),
    );

    fn private_source() -> CGEventSource {
        CGEventSource::new(CGEventSourceStateID::Private).expect("a private source")
    }

    // HOME/END/PAGE_UP/PAGE_DOWN read like the arrows and do not carry NumericPad.
    #[test]
    fn only_the_arrows_and_the_keypad_are_intrinsically_numeric_pad() {
        for code in [
            KeyCode::UP_ARROW,
            KeyCode::LEFT_ARROW,
            KeyCode::ANSI_KEYPAD_ENTER,
            KeyCode::ANSI_KEYPAD_7,
        ] {
            assert_eq!(
                intrinsic_flags(code),
                CGEventFlags::CGEventFlagNumericPad,
                "keycode {code} carries NumericPad on a clean source"
            );
        }
        for code in [
            KeyCode::HOME,
            KeyCode::END,
            KeyCode::PAGE_UP,
            KeyCode::PAGE_DOWN,
            KeyCode::ANSI_KEYPAD_CLEAR,
            KeyCode::SPACE,
            KeyCode::ANSI_A,
        ] {
            assert_eq!(
                intrinsic_flags(code),
                CGEventFlags::empty(),
                "keycode {code} carries nothing outside MODIFIERS"
            );
        }
    }

    // Exact equality: a reintroduced `| (get_flags() & !MODIFIERS)` fails here rather than
    // in Spotlight later.
    #[test]
    fn the_wire_flags_are_the_portable_ones_plus_the_intrinsic_ones() {
        let source = private_source();
        for (key, code, flags) in [
            (Key::Space, KeyCode::SPACE, ModifierFlags::COMMAND),
            (Key::UpArrow, KeyCode::UP_ARROW, ModifierFlags::empty()),
            (Key::UpArrow, KeyCode::UP_ARROW, ModifierFlags::COMMAND),
            (Key::KeyR, KeyCode::ANSI_R, ModifierFlags::CONTROL),
        ] {
            let event =
                keyboard_event(&source, key, PressType::Down, flags).expect("an event for the key");
            assert_eq!(
                event.get_flags(),
                to_cg(flags) | intrinsic_flags(code),
                "{key:?} with {flags:?}"
            );
        }
    }

    // An arrow must not leave NumericPad on a later `cmd`-`space`.
    #[test]
    fn a_chord_carries_its_modifier_and_nothing_else() {
        let space = keyboard_event(
            &private_source(),
            Key::Space,
            PressType::Down,
            ModifierFlags::COMMAND,
        )
        .expect("a space");
        assert_eq!(
            space.get_flags() & MODIFIERS,
            CGEventFlags::CGEventFlagCommand
        );
        assert!(
            !space
                .get_flags()
                .contains(CGEventFlags::CGEventFlagNumericPad)
        );
    }

    // An arrow keeps NumericPad; a space never gains it.
    #[test]
    fn a_keys_own_flags_survive_and_others_do_not_appear() {
        let source = private_source();
        let arrow = keyboard_event(
            &source,
            Key::UpArrow,
            PressType::Down,
            ModifierFlags::empty(),
        )
        .expect("an arrow");
        let space = keyboard_event(&source, Key::Space, PressType::Down, ModifierFlags::empty())
            .expect("a space");
        assert!(
            arrow
                .get_flags()
                .contains(CGEventFlags::CGEventFlagNumericPad)
        );
        assert!(
            !space
                .get_flags()
                .contains(CGEventFlags::CGEventFlagNumericPad)
        );
    }

    // A keypad key arrives as `Key::Raw` and must still post NumericPad.
    #[test]
    fn a_raw_keypad_key_keeps_its_numeric_pad_bit() {
        let event = keyboard_event(
            &private_source(),
            Key::Raw(KeyCode::ANSI_KEYPAD_ENTER),
            PressType::Down,
            ModifierFlags::empty(),
        )
        .expect("a keypad enter");
        assert_eq!(event.get_flags(), CGEventFlags::CGEventFlagNumericPad);
    }

    // A remapped key carries the flags it was given.
    #[test]
    fn a_remapped_key_carries_the_flags_it_was_given() {
        let event = keyboard_event(
            &private_source(),
            Key::KeyR,
            PressType::Down,
            ModifierFlags::COMMAND,
        )
        .expect("a key");
        assert!(event.get_flags().contains(CGEventFlags::CGEventFlagCommand));
    }

    // The OS picks the type from the keycode.
    #[test]
    fn a_modifier_is_a_flags_changed_and_a_key_is_not() {
        let source = private_source();
        let cmd = keyboard_event(
            &source,
            Key::MetaLeft,
            PressType::Down,
            ModifierFlags::COMMAND,
        )
        .expect("cmd");
        let space = keyboard_event(&source, Key::Space, PressType::Down, ModifierFlags::empty())
            .expect("a space");
        assert!(matches!(cmd.get_type(), CGEventType::FlagsChanged));
        assert!(matches!(space.get_type(), CGEventType::KeyDown));
    }

    #[test]
    fn a_key_with_no_code_is_unmappable() {
        assert!(matches!(
            keyboard_event(
                &private_source(),
                Key::F24,
                PressType::Down,
                ModifierFlags::empty()
            ),
            Err(EmitError::Unmappable(Key::F24))
        ));
    }

    // The tag must mark an event it stamped and no other.
    #[test]
    fn a_tag_marks_only_its_own_events() {
        let source = CGEventSource::new(CGEventSourceStateID::Private).expect("a private source");
        let event =
            CGEvent::new_keyboard_event(source, KeyCode::SPACE, true).expect("a keyboard event");
        let (mine, theirs) = (Tag::new(), Tag(1));
        assert!(!mine.marks(&event));
        mine.stamp(&event);
        assert!(mine.marks(&event));
        assert!(!theirs.marks(&event));
    }

    #[test]
    fn flags_changed_one_side_release_while_the_other_is_held_is_up() {
        for (left, right) in [
            (Key::ShiftLeft, Key::ShiftRight),
            (Key::ControlLeft, Key::ControlRight),
            (Key::AltLeft, Key::AltRight),
            (Key::MetaLeft, Key::MetaRight),
        ] {
            let down = FlagsChangedDown::new();
            assert_eq!(
                press_of_key(CGEventType::FlagsChanged, left, &down),
                Some(PressType::Down)
            );
            assert_eq!(
                press_of_key(CGEventType::FlagsChanged, right, &down),
                Some(PressType::Down)
            );
            assert_eq!(
                press_of_key(CGEventType::FlagsChanged, left, &down),
                Some(PressType::Up),
                "{left:?} released while {right:?} held"
            );
            assert_eq!(
                press_of_key(CGEventType::FlagsChanged, right, &down),
                Some(PressType::Up)
            );
        }
    }

    #[test]
    fn flags_changed_caps_lock_toggles() {
        let down = FlagsChangedDown::new();
        assert_eq!(
            press_of_key(CGEventType::FlagsChanged, Key::CapsLock, &down),
            Some(PressType::Down)
        );
        assert_eq!(
            press_of_key(CGEventType::FlagsChanged, Key::CapsLock, &down),
            Some(PressType::Up)
        );
        assert_eq!(
            press_of_key(CGEventType::FlagsChanged, Key::CapsLock, &down),
            Some(PressType::Down)
        );
    }

    #[test]
    fn flags_changed_of_a_non_modifier_is_none() {
        let down = FlagsChangedDown::new();
        assert_eq!(
            press_of_key(CGEventType::FlagsChanged, Key::KeyA, &down),
            None
        );
    }

    #[test]
    fn flags_changed_bits_cover_each_modifier_side_and_caps_lock() {
        for key in [
            Key::ShiftLeft,
            Key::ShiftRight,
            Key::ControlLeft,
            Key::ControlRight,
            Key::AltLeft,
            Key::AltRight,
            Key::MetaLeft,
            Key::MetaRight,
            Key::CapsLock,
        ] {
            assert!(flags_changed_bit(key).is_some(), "{key:?}");
        }
        assert_eq!(flags_changed_bit(Key::KeyA), None);
        assert_eq!(flags_changed_bit(Key::Escape), None);
    }
}
