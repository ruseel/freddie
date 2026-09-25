# Button Decks automation bridge

## External contract

Button Decks sends a raw UTF-8 Clojure s-expression to an unauthenticated Clojure HTTP receiver. The receiver parses only these forms and forwards their JSON representation over a loopback-only Mercury bridge:

```clojure
;; from receiver/src/button_decks/receiver.clj
(tap :key-r [:command])
(emit :key-a :down [])
(foreground "com.google.Chrome")
```

The HTTP receiver owns the LAN boundary. Mercury accepts newline-delimited JSON only at `127.0.0.1:3884`; it never accepts a LAN connection and it never receives Clojure source. Each command has one line and one response line:

```json
{"kind":"AutomationCommand.Tap","value":{"key":"key-r","modifiers":["command"]}}
{"state":"confirmed"}
```

## New runtime types

```rust
// from crates/mercury/src/automation_socket.rs
pub(crate) enum AutomationCommand {
    Tap(AutomationTap),
    Emit(AutomationEmit),
    Foreground(AutomationForeground),
}

pub(crate) struct AutomationTap {
    pub(crate) key: Key,
    pub(crate) flags: ModifierFlags,
}

pub(crate) struct AutomationEmit {
    pub(crate) key: Key,
    pub(crate) press: PressType,
    pub(crate) flags: ModifierFlags,
}

pub(crate) struct AutomationForeground {
    pub(crate) bundle_id: String,
}
```

```rust
// from crates/mercury/src/daemon.rs
enum RuntimeEffect {
    Mercury(MercuryEffect),
    Automation(AutomationCommand),
}
```

`AutomationCommand` is not part of the pure model. The local bridge sends it to the runtime effect channel; the existing model continues to send `MercuryEffect` values to that same receiver. `run_effect_loop` serializes both kinds. Tap and emit use the daemon's existing `Emitter`. Foreground calls `freddie_app_nav::foreground` from the same effect sink and reports only dispatch acceptance, not a verified frontmost app.

## Ordered changes

1. Add `automation_socket.rs`. It binds `127.0.0.1:<port>`, limits one line to 16 KiB, parses the three tagged JSON command variants, converts only explicit key/modifier names, and sends a command over the existing effect channel. Invalid input receives `{"state":"rejected","message":"..."}` and causes no effect.
2. Change the daemon channel to carry `RuntimeEffect`, start the local automation socket before keyboard interception, and execute external commands in the same effect loop as model effects. Add `--automation-port` / `MERCURY_AUTOMATION_PORT`, defaulting to 3884.
3. Add Clojure `receiver/` files in Button Decks. The HTTP server accepts `POST /repl`, parses the closed form grammar above without `eval`, and forwards it to Mercury's loopback bridge. Its bind address defaults to `0.0.0.0` for iPad access and it has no authentication by explicit product decision.
4. Change the iPad client to send the exact expression as `text/plain; charset=utf-8` to `http://<mac>:7777/repl`, without a token or automatic retry. The app treats a `confirmed` receiver response as command dispatch confirmation.

## Verification

- Mercury unit tests cover valid JSON conversion, malformed/oversized input rejection, and no mapping for disallowed commands.
- Clojure tests cover the s-expression grammar and an end-to-end loopback bridge fixture.
- Button Decks transport tests assert HTTP, raw verbatim request body, no `Authorization` header, `/repl`, and no retry on timeout.
- A manual macOS run sends `(tap :key-r [:command])` and `(foreground "com.apple.TextEdit")` through the Clojure HTTP receiver while Mercury is running with Accessibility permission.
