# mouse chord key

Logitech Master 3S의 back/forward 사이드 버튼을 chord modifier로 사용: 버튼을 누른 채 키보드 키(f, d, s, a 등)를 누르면 바인딩이 발동한다. 버튼 단독 클릭은 원래 back/forward로 재생한다.

## 전체 설계

마우스 사이드 버튼은 macOS에서 `CGEventType::OtherMouseDown` / `OtherMouseUp`으로 들어온다. button number 3 = back, 4 = forward. 현재 mercury의 `CGEventTap`은 키보드만 감시하므로 마우스 버튼도 감시하도록 확장한다.

모델에서는 사이드 버튼의 held 상태를 `Mercury` 루트에 기록하고, 키보드 키 이벤트가 들어왔을 때 held 상태와 결합하여 chord trigger를 만든다. 사이드 버튼은 swallow하되, 키보드 키 없이 단독으로 놓이면 (tap) 원래 back/forward를 재생한다.

## 변경 1: `freddie_keys`에 `MouseButton` 추가

`freddie_keys`는 platform-neutral 입력 어휘 크레이트이므로 마우스 버튼도 여기에 둔다.

```rust
// from crates/freddie_keys/src/lib.rs

/// A physical mouse button, named by its HID function.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseButton {
    /// Button 3 on macOS: the "back" thumb button.
    Back,
    /// Button 4 on macOS: the "forward" thumb button.
    Forward,
}

/// A mouse button going down or coming up.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MouseButtonEvent {
    pub button: MouseButton,
    pub press: PressType,
}
```

`MouseButton`은 `Key`와 대등하지 않다. `Key`의 variant로 넣으면 키보드 바인딩 전체가 마우스를 알아야 하고, `AnyKey` 같은 catch-all이 마우스까지 삼키게 된다. 별도 타입으로 두고 이벤트와 트리거도 분리한다.

## 변경 2: `MercuryEvent::MouseButton` 변형 추가

```rust
// from crates/mercury_model/src/model.rs

#[derive(Debug, derive_more::TryInto)]
#[try_into(ref)]
pub enum MercuryEvent {
    Key(KeyEvent),
    MouseButton(MouseButtonEvent),  // 추가
    Foreground(ForegroundEvent),
    Tab(TabEvent),
    Window(WindowEvent),
    FrameRead(FrameRead),
    FocusRead(FocusRead),
    Quit(Quit),
    Timer(TimerFired),
}
```

## 변경 3: `MercuryTrigger`에 `MouseButtonPressed` 추가

```rust
// from crates/mercury_model/src/model.rs

#[derive(Clone, PartialEq, Eq, Hash, Debug, derive_more::From)]
pub enum MercuryTrigger {
    Key(Key),
    KeyPress(KeyPress),
    KeyChord(KeyChord),
    AnyKey(AnyKey),
    MouseButtonPressed(MouseButtonPressed),  // 추가
    Foregrounded(Foregrounded),
    Tabbed(Tabbed),
    Windowed(Windowed),
    FrameLanded(FrameLanded),
    FocusLanded(FocusLanded),
    Quit(Quit),
}
```

`MouseButtonPressed` trigger:

```rust
// from crates/mercury_model/src/sources.rs

/// Matches any mouse button event.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MouseButtonPressed;

impl EventTrigger for MouseButtonPressed {
    type Event = MouseButtonEvent;
    fn is_matching(&self, _ev: &MouseButtonEvent) -> bool {
        true
    }
}
```

## 변경 4: `Mercury` 루트에 held mouse button 상태 + 핸들러

```rust
// from crates/mercury_model/src/state/mod.rs

#[derive(Bind, Debug)]
#[node(root)]
#[binds(MercuryStruct)]
#[bind(
    // ... 기존 바인딩들 ...
    MouseButtonPressed => if_not_invalidated(record_mouse_button),
)]
// ... 기존 #[bind] 및 #[post] ...
pub struct Mercury {
    // ... 기존 필드들 ...
    /// Mouse side buttons currently held. Tracked for chord detection.
    pub mouse_held: HeldMouseButtons,
}
```

```rust
// from crates/mercury_model/src/state/mod.rs

/// Which mouse side buttons are physically held.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeldMouseButtons {
    pub back: MouseButtonHoldState,
    pub forward: MouseButtonHoldState,
}

/// Whether a mouse button is released, held (waiting for chord), or already used as chord.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonHoldState {
    #[default]
    Released,
    /// Down, no keyboard key yet. If released in this state, replay the original button.
    Held,
    /// A keyboard chord consumed this hold. Do not replay on release.
    Chorded,
}
```

핸들러:

```rust
// from crates/mercury_model/src/handlers/root.rs

use freddie_keys::{MouseButton, MouseButtonEvent, PressType};
use crate::state::{MouseButtonHoldState, HeldMouseButtons};

/// Record mouse button down/up. On down: swallow and mark Held.
/// On up: if still Held (no chord happened), replay the button as effect.
/// If Chorded, just release silently.
pub(crate) fn record_mouse_button<'a>(
    ev: &MouseButtonEvent,
    _snap: (),
    p: MercuryPath<'a>,
) -> (Vec<MercuryEffect>, Completed<MercuryPath<'a>>) {
    let state = match ev.button {
        MouseButton::Back => &mut p.mouse_held.back,
        MouseButton::Forward => &mut p.mouse_held.forward,
    };
    let effects = match ev.press {
        PressType::Down => {
            *state = MouseButtonHoldState::Held;
            Vec::new()
        }
        PressType::Up => {
            let was = *state;
            *state = MouseButtonHoldState::Released;
            if was == MouseButtonHoldState::Held {
                // 단독 tap: 원래 버튼 재생
                vec![MercuryEffect::MouseButtonTap(ev.button)]
            } else {
                // Chorded: 이미 사용됨, 아무것도 안 함
                Vec::new()
            }
        }
    };
    (effects, p.complete())
}
```

## 변경 5: `MercuryEffect::MouseButtonTap` 추가

단독 tap 시 원래 back/forward를 OS에 재생하기 위한 effect.

```rust
// from crates/mercury_model/src/effect.rs

#[derive(Debug)]
pub enum MercuryEffect {
    // ... 기존 variant들 ...
    /// Replay a mouse side button tap (down+up). Used when the button was held
    /// but no keyboard chord consumed it.
    MouseButtonTap(MouseButton),
}
```

## 변경 6: keyboard chord 매칭에서 mouse held 상태 활용

핵심: 마우스 사이드 버튼이 held 상태일 때 키보드 키가 내려오면, 일반 바인딩 대신 mouse-chord 바인딩을 발동해야 한다.

`Mercury::handle()`을 확장하여 mouse-chord를 먼저 확인한다.

```rust
// from crates/mercury_model/src/state/mod.rs

impl Mercury {
    #[must_use]
    pub fn handle(&mut self, event: &MercuryEvent) -> Vec<MercuryEffect> {
        // Mouse chord intercept: if a mouse side button is held and a key goes down,
        // look up the chord binding. If found, mark the button as Chorded and return
        // the chord's effects instead of normal dispatch.
        if let MercuryEvent::Key(key_event) = event {
            if key_event.press == PressType::Down {
                if let Some(effects) = self.try_mouse_chord(key_event) {
                    return effects;
                }
            }
        }
        bind::dispatch::<MercuryStruct, Self, _>(self, event)
    }

    fn try_mouse_chord(&mut self, key_event: &KeyEvent) -> Option<Vec<MercuryEffect>> {
        let held_button = if self.mouse_held.back == MouseButtonHoldState::Held
            || self.mouse_held.back == MouseButtonHoldState::Chorded
        {
            Some(MouseButton::Back)
        } else if self.mouse_held.forward == MouseButtonHoldState::Held
            || self.mouse_held.forward == MouseButtonHoldState::Chorded
        {
            Some(MouseButton::Forward)
        } else {
            None
        }?;

        let effects = mouse_chord_binding(held_button, key_event.key)?;

        // Mark as chorded so release won't replay the button
        match held_button {
            MouseButton::Back => self.mouse_held.back = MouseButtonHoldState::Chorded,
            MouseButton::Forward => self.mouse_held.forward = MouseButtonHoldState::Chorded,
        }

        Some(effects)
    }
}
```

chord 바인딩 테이블:

```rust
// from crates/mercury_model/src/state/mod.rs

/// Mouse chord bindings. Back+key and Forward+key mappings.
/// Returns effects for the chord, or None if no binding exists for this key.
fn mouse_chord_binding(button: MouseButton, key: Key) -> Option<Vec<MercuryEffect>> {
    match (button, key) {
        // Back button chords — same as Nav layer app switching
        (MouseButton::Back, Key::KeyF) => Some(vec![MercuryEffect::Foreground(App::Ghostty)]),
        (MouseButton::Back, Key::KeyD) => Some(vec![MercuryEffect::Foreground(App::Obsidian)]),
        (MouseButton::Back, Key::KeyS) => Some(vec![MercuryEffect::Foreground(App::Codex)]),
        (MouseButton::Back, Key::KeyA) => Some(vec![MercuryEffect::Foreground(App::Zed)]),
        (MouseButton::Back, Key::KeyC) => Some(vec![MercuryEffect::Foreground(App::Chrome)]),
        // Forward button chords — reserved, no bindings yet
        _ => None,
    }
}
```

## 변경 7: `freddie_keyboard` CGEventTap 확장 — 마우스 사이드 버튼 감시

별도의 마우스 tap을 설치한다. 기존 `intercept` 시그니처를 깨뜨리지 않기 위해 새 함수를 추가한다.

```rust
// from crates/freddie_keyboard/src/sys/macos.rs

/// Which mouse button number maps to which MouseButton.
const MOUSE_BUTTON_BACK: i64 = 3;
const MOUSE_BUTTON_FORWARD: i64 = 4;

fn mouse_button_number(event: &CGEvent) -> i64 {
    event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER)
}

/// Intercept mouse side buttons (Other mouse buttons 3 and 4).
/// `on_button` receives the event; returning `None` swallows it.
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
                let button = match button_num {
                    MOUSE_BUTTON_BACK => MouseButton::Back,
                    MOUSE_BUTTON_FORWARD => MouseButton::Forward,
                    _ => return CallbackResult::Keep, // 다른 마우스 버튼은 통과
                };
                let press = match kind {
                    CGEventType::OtherMouseDown => PressType::Down,
                    CGEventType::OtherMouseUp => PressType::Up,
                    _ => return CallbackResult::Keep,
                };
                let input = MouseButtonEvent { button, press };
                match on_button(input) {
                    None => CallbackResult::Drop,
                    Some(_) => CallbackResult::Keep,
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
    Ok(MouseInterceptor {
        _tap: TapThread {
            run_loop,
            thread: Some(thread),
        },
    })
}

pub struct MouseInterceptor {
    _tap: TapThread,
}
```

## 변경 8: `daemon.rs`에서 마우스 tap 설치 및 effect 처리

```rust
// from crates/mercury/src/daemon.rs — serve() 함수 안

// 기존 keyboard intercept 아래에 추가:
let mouse_grabbed = freddie_keyboard::intercept_mouse(emitter.tag(), {
    let event_tx = event_tx.clone();
    move |ev| {
        let _ = event_tx.send(MercuryEvent::MouseButton(ev));
        None // swallow
    }
});
let mouse_interceptor = match mouse_grabbed {
    Ok(interceptor) => Some(interceptor),
    Err(e) => {
        warn!(error = %e, "could not intercept mouse side buttons");
        None
    }
};
```

effect 처리:

```rust
// from crates/mercury/src/daemon.rs — perform_effect() 함수 안

MercuryEffect::MouseButtonTap(button) => match emitter.tap_mouse(button) {
    Ok(()) => debug!(?button, "mouse button tapped"),
    Err(e) => warn!(?button, error = %e, "mouse button tap failed"),
},
```

```rust
// from crates/freddie_keyboard/src/sys/macos.rs

impl Emitter {
    /// Press then release `button`.
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
}
```

## 변경 순서

1. `freddie_keys`: `MouseButton`, `MouseButtonEvent` 타입 추가
2. `mercury_model/sources.rs`: `MouseButtonPressed` trigger 추가
3. `mercury_model/model.rs`: `MercuryEvent::MouseButton`, `MercuryTrigger::MouseButtonPressed` 추가
4. `mercury_model/effect.rs`: `MercuryEffect::MouseButtonTap` 추가
5. `mercury_model/state/mod.rs`: `HeldMouseButtons`, `MouseButtonHoldState` 추가, `Mercury` 필드 추가
6. `mercury_model/handlers/root.rs`: `record_mouse_button` 핸들러 추가
7. `mercury_model/state/mod.rs`: `Mercury::handle()` 확장 — `try_mouse_chord` 추가
8. `freddie_keyboard`: `intercept_mouse` 및 `Emitter::tap_mouse` 추가
9. `mercury/daemon.rs`: 마우스 tap 설치, `MouseButtonTap` effect 처리
