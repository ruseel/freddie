//! Per-event tests call `handle` once. The loop tests drive a `bind::SimpleRunner` and report a `Foreground` effect back as a foreground event.

use bind::SimpleRunner;
use freddie_windows_types::{Frame, Monitor, WindowChange, WindowId};
use mercury_model::{
    App, Chord, HomeLayer, JK_TIMEOUT, Key, KeyEvent, Layer, Mercury, MercuryEffect, MercuryEvent,
    MercuryStruct, ModifierFlags, OVERLAY_DWELL, PLACEMENT_SETTLE, PressType,
    RETURN_TO_HOME_TIMEOUT, ReturnHomeLayers, WindowEvent, Windows, focus_read, foreground,
    frame_read, key, quit_event, tab,
};
use mercury_model::{FrontApp, Pid, Placement};

// `BOOT_TITLE` is a literal painted before the model exists; this keeps the literal honest.
#[test]
fn boot_title_matches_the_boot_layer() {
    let booted = Mercury::new(Some(FrontApp::new(App::Other, Pid(1))), Windows::default());
    assert_eq!(booted.layer().name(), Mercury::BOOT_TITLE);
}

fn front(m: &Mercury) -> Option<App> {
    m.foreground.as_ref().map(|front| front.app.identity())
}

// Equality under `testing` compares delay and fire event, so a rebuilt timer matches what a layer produced.
fn return_home_timer() -> MercuryEffect {
    let (_guard, effect) = freddie::timer_effect_and_guard(RETURN_TO_HOME_TIMEOUT, ());
    MercuryEffect::Timer(effect)
}

fn settle_timer() -> MercuryEffect {
    let (_guard, effect) = freddie::timer_effect_and_guard(PLACEMENT_SETTLE, ());
    MercuryEffect::Timer(effect)
}

fn timer_event(effects: &[MercuryEffect]) -> MercuryEvent {
    effects
        .iter()
        .find_map(|e| match e {
            MercuryEffect::Timer(timer) => Some(MercuryEvent::Timer(timer.event)),
            _ => None,
        })
        .expect("these effects set a timer")
}

const fn return_home(m: &Mercury) -> Option<&ReturnHomeLayers> {
    match m.layer() {
        Layer::ReturnHome(w) => Some(w.layers()),
        Layer::Home(_) | Layer::Typing(_) => None,
    }
}

// Most per-event tests exercise Home's command bindings; the default is Typing.
fn home() -> Mercury {
    let mut m = Mercury::with_layer(Layer::Home(HomeLayer));
    let _ = m.handle(&foreground(App::Other, Pid(1)));
    m
}

const fn emit(key: Key, press: PressType) -> MercuryEffect {
    emit_with(key, press, ModifierFlags::empty())
}

const fn emit_with(key: Key, press: PressType, flags: ModifierFlags) -> MercuryEffect {
    MercuryEffect::Emit(KeyEvent { key, press, flags })
}

fn passed(key: Key) -> Vec<MercuryEffect> {
    vec![emit(key, PressType::Down)]
}

const fn up(key: Key) -> MercuryEvent {
    MercuryEvent::Key(KeyEvent {
        key,
        press: PressType::Up,
        flags: ModifierFlags::empty(),
    })
}

const fn key_with(key: Key, flags: ModifierFlags) -> MercuryEvent {
    MercuryEvent::Key(KeyEvent {
        key,
        press: PressType::Down,
        flags,
    })
}

const fn tap(key: Key, flags: ModifierFlags) -> MercuryEffect {
    MercuryEffect::Tap(Chord { key, flags })
}

fn cmd_r() -> Vec<MercuryEffect> {
    vec![tap(Key::KeyR, ModifierFlags::COMMAND)]
}

const fn shows(layer: &'static str) -> MercuryEffect {
    MercuryEffect::ShowLayer(layer)
}

// The go-home name comes last: the handler emits its own effects before it changes the layer.
fn leaves(mut effects: Vec<MercuryEffect>) -> Vec<MercuryEffect> {
    effects.push(shows("Home"));
    effects
}

// A stay in the in-app layer resets the return-home timer. Keys that leave do not, so they use the bare effects.
fn in_app(mut effects: Vec<MercuryEffect>) -> Vec<MercuryEffect> {
    effects.push(return_home_timer());
    effects
}

#[test]
fn default_boots_into_typing() {
    assert!(matches!(
        Mercury::new(Some(FrontApp::new(App::Other, Pid(1))), Windows::default()).layer(),
        Layer::Typing(_)
    ));
}

#[test]
fn every_layer_has_a_name_for_the_menu_bar() {
    let mut m = home();
    assert_eq!(m.layer().name(), "Home");
    let _ = m.handle(&key(Key::KeyN));
    assert_eq!(m.layer().name(), "Nav");
    let _ = m.handle(&key(Key::Escape));
    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(m.layer().name(), "Resize");
    let _ = m.handle(&key(Key::Escape));
    let _ = m.handle(&key(Key::KeyT));
    assert_eq!(m.layer().name(), "Typing");

    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(m.layer().name(), "App");
}

#[test]
fn home_n_enters_nav() {
    let mut m = home();
    assert_eq!(
        m.handle(&key(Key::KeyN)),
        vec![shows("Nav"), return_home_timer()]
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))));
}

#[test]
fn home_t_enters_typing() {
    let mut m = home();
    assert_eq!(m.handle(&key(Key::KeyT)), vec![shows("Typing")]);
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn home_q_quits() {
    let mut m = home();
    assert_eq!(m.handle(&key(Key::KeyQ)), vec![MercuryEffect::Kill]);
}

#[test]
fn f1_foregrounds_obsidian_and_enters_typing_from_home() {
    let mut m = home();
    assert_eq!(
        m.handle(&key(Key::F1)),
        vec![MercuryEffect::Foreground(App::Obsidian), shows("Typing")]
    );
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn f1_foregrounds_obsidian_from_typing() {
    let mut m = Mercury::new(Some(FrontApp::new(App::Other, Pid(1))), Windows::default());
    assert_eq!(
        m.handle(&key(Key::F1)),
        vec![MercuryEffect::Foreground(App::Obsidian), shows("Typing")]
    );
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn f2_and_f3_foreground_their_apps_from_home() {
    for (function_key, app) in [(Key::F2, App::Ghostty), (Key::F3, App::Chrome)] {
        let mut m = home();
        assert_eq!(
            m.handle(&key(function_key)),
            vec![MercuryEffect::Foreground(app), shows("Typing")]
        );
        assert!(matches!(m.layer(), Layer::Typing(_)));
    }
}

#[test]
fn quit_event_kills_from_home() {
    let mut m = home();
    assert_eq!(m.handle(&quit_event()), vec![MercuryEffect::Kill]);
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn quit_emits_held_modifiers_so_the_app_learns_the_physical_state() {
    // cmd held in home is swallowed, so the app never saw its down. On quit the grab is released and no further down is coming, so emit the down before Kill.
    let mut m = home();
    let _ = m.handle(&key(Key::MetaLeft));
    assert_eq!(
        m.handle(&quit_event()),
        vec![
            emit_with(Key::MetaLeft, PressType::Down, ModifierFlags::COMMAND),
            MercuryEffect::Kill,
        ]
    );
}

#[test]
fn quit_event_kills_from_every_layer() {
    // Typing's `AnyKey` catch-all must not swallow Quit (a different event type), so it still reaches the root.
    for enter in [Key::KeyN, Key::KeyT, Key::KeyR, Key::KeyI] {
        let mut m = home();
        let _ = m.handle(&key(enter));
        assert_eq!(
            m.handle(&quit_event()),
            vec![MercuryEffect::Kill],
            "quit from the layer entered by {enter:?}"
        );
    }
}

#[test]
fn home_escape_does_nothing() {
    let mut m = home();
    assert_eq!(m.handle(&key(Key::Escape)), vec![shows("Home")]);
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn escape_goes_home_from_a_sublayer() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyN));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))));
    // The deadline post rearms during descent; go_home then drops the layer and its guard, cancelling the rearmed timer.
    assert_eq!(
        m.handle(&key(Key::Escape)),
        vec![return_home_timer(), shows("Home")]
    );
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn nav_times_out_home() {
    let mut m = home();
    let entered = m.handle(&key(Key::KeyN));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))));
    assert_eq!(m.handle(&timer_event(&entered)), vec![shows("Home")]);
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn a_firing_from_a_layer_already_left_matches_nothing() {
    // First timer's firing arrives after a second nav replaced it. It must not send the live one home.
    let mut m = home();
    let first = timer_event(&m.handle(&key(Key::KeyN)));
    let _ = m.handle(&key(Key::Escape));
    let second = timer_event(&m.handle(&key(Key::KeyN)));

    assert_eq!(
        m.handle(&first),
        vec![],
        "no binding matches a stale firing"
    );
    assert!(
        matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))),
        "still in nav"
    );

    assert_eq!(m.handle(&second), vec![shows("Home")]);
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn a_firing_in_a_layer_that_set_no_timer_matches_nothing() {
    let mut m = home();
    let stale = timer_event(&m.handle(&key(Key::KeyN)));
    let _ = m.handle(&key(Key::Escape));
    assert_eq!(m.handle(&stale), vec![]);
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn typing_passes_any_key_through() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyT));
    assert_eq!(m.handle(&key(Key::KeyA)), passed(Key::KeyA));
    assert_eq!(m.handle(&key(Key::KeyZ)), passed(Key::KeyZ));
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn typing_passes_a_baked_modifier_through() {
    // A modifier baked onto the event (injected cmd-v, or fn) rides through instead of being dropped.
    let mut m = home();
    let _ = m.handle(&key(Key::KeyT));
    let cmd_v = MercuryEvent::Key(KeyEvent {
        key: Key::KeyV,
        press: PressType::Down,
        flags: ModifierFlags::COMMAND,
    });
    assert_eq!(
        m.handle(&cmd_v),
        vec![emit_with(
            Key::KeyV,
            PressType::Down,
            ModifierFlags::COMMAND
        )]
    );
}

#[test]
fn typing_plain_escape_passes_through() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyT));
    assert_eq!(m.handle(&key(Key::Escape)), passed(Key::Escape));
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn typing_cmd_escape_types_the_escape() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyT));

    // flagsChanged arrives carrying the command flag. Tracked in held and passed through with that flag.
    let cmd_down = MercuryEvent::Key(KeyEvent {
        key: Key::MetaLeft,
        press: PressType::Down,
        flags: ModifierFlags::COMMAND,
    });
    assert_eq!(
        m.handle(&cmd_down),
        vec![emit_with(
            Key::MetaLeft,
            PressType::Down,
            ModifierFlags::COMMAND
        )]
    );

    let cmd_escape = MercuryEvent::Key(KeyEvent {
        key: Key::Escape,
        press: PressType::Down,
        flags: ModifierFlags::COMMAND,
    });
    assert_eq!(
        m.handle(&cmd_escape),
        vec![emit_with(
            Key::Escape,
            PressType::Down,
            ModifierFlags::COMMAND
        )]
    );
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn nav_c_foregrounds_chrome_and_enters_inapp() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyN));
    assert_eq!(
        m.handle(&key(Key::KeyC)),
        vec![
            MercuryEffect::Foreground(App::Chrome),
            shows("App"),
            return_home_timer(),
        ]
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), None);
}

#[test]
fn every_nav_choice_enters_inapp() {
    for (k, app) in [
        (Key::KeyA, App::Zed),
        (Key::KeyC, App::Chrome),
        (Key::KeyD, App::Obsidian),
        (Key::KeyF, App::Ghostty),
        (Key::KeyS, App::Codex),
    ] {
        let mut m = home();
        let _ = m.handle(&key(Key::KeyN));
        assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))));
        assert_eq!(
            m.handle(&key(k)),
            vec![
                MercuryEffect::Foreground(app),
                shows("App"),
                return_home_timer(),
            ]
        );
        assert!(
            matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))),
            "{app:?} left nav"
        );
        assert!(front(&m).is_none(), "{app:?} did not mark the nav pending");
    }
}

#[test]
fn nav_space_opens_spotlight_and_enters_typing() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyN));
    assert_eq!(
        m.handle(&key(Key::Space)),
        vec![tap(Key::Space, ModifierFlags::COMMAND), shows("Typing")]
    );
    assert!(matches!(m.layer(), Layer::Typing(_)));
    assert_eq!(front(&m), Some(App::Other));
    assert_eq!(m.handle(&key(Key::KeyC)), passed(Key::KeyC));
}

#[test]
fn n_c_then_foreground_then_r_refreshes_chrome() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyN));
    assert_eq!(
        m.handle(&key(Key::KeyC)),
        vec![
            MercuryEffect::Foreground(App::Chrome),
            shows("App"),
            return_home_timer(),
        ]
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));

    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    assert_eq!(front(&m), Some(App::Chrome));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(cmd_r()));
}

#[test]
fn a_pending_nav_binds_nothing_until_the_foreground_event() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyN));
    let _ = m.handle(&key(Key::KeyC));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), None);
    assert_eq!(m.handle(&key(Key::KeyJ)), in_app(vec![]));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(vec![]));

    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Chrome));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(cmd_r()));
}

#[test]
fn foreground_records_the_app_without_changing_layer() {
    let mut m = home();
    assert_eq!(m.handle(&foreground(App::Zed, Pid(7))), vec![]);
    assert_eq!(front(&m), Some(App::Zed));
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn i_enters_inapp_for_the_foregrounded_app() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    assert_eq!(
        m.handle(&key(Key::KeyI)),
        vec![shows("App"), return_home_timer()]
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Chrome));
}

fn chrome_showing(url: &str) -> Mercury {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    let _ = m.handle(&tab(url.to_owned()));
    m
}

fn copies(text: &str) -> MercuryEffect {
    MercuryEffect::Copy(text.to_owned())
}

#[test]
fn chrome_l_focuses_the_address_bar_and_enters_typing() {
    let mut m = chrome_showing("https://www.x.com/asdfasdf");
    assert_eq!(
        m.handle(&key(Key::KeyL)),
        vec![tap(Key::KeyL, ModifierFlags::COMMAND), shows("Typing")]
    );
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn chrome_shift_l_copies_the_url() {
    let mut m = chrome_showing("https://www.x.com/asdfasdf");
    assert_eq!(
        m.handle(&key_with(Key::KeyL, ModifierFlags::SHIFT)),
        in_app(vec![copies("https://www.x.com/asdfasdf")])
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
}

#[test]
fn chrome_cmd_l_copies_the_host() {
    let mut m = chrome_showing("https://www.x.com/asdfasdf");
    assert_eq!(
        m.handle(&key_with(Key::KeyL, ModifierFlags::COMMAND)),
        in_app(vec![copies("www.x.com")])
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
}

#[test]
fn the_three_ls_do_not_shadow_each_other() {
    for (event, want) in [
        (
            key(Key::KeyL),
            vec![tap(Key::KeyL, ModifierFlags::COMMAND), shows("Typing")],
        ),
        (
            key_with(Key::KeyL, ModifierFlags::SHIFT),
            in_app(vec![copies("https://claude.ai/new")]),
        ),
        (
            key_with(Key::KeyL, ModifierFlags::COMMAND),
            in_app(vec![copies("claude.ai")]),
        ),
    ] {
        let mut m = chrome_showing("https://claude.ai/new");
        assert_eq!(m.handle(&event), want, "{event:?}");
    }
}

#[test]
fn a_copy_with_no_reported_url_copies_nothing() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(
        m.handle(&key_with(Key::KeyL, ModifierFlags::SHIFT)),
        in_app(vec![])
    );
    assert_eq!(
        m.handle(&key_with(Key::KeyL, ModifierFlags::COMMAND)),
        in_app(vec![])
    );
}

#[test]
fn copying_the_host_of_a_hostless_url_copies_nothing() {
    let mut m = chrome_showing("about:blank");
    assert_eq!(
        m.handle(&key_with(Key::KeyL, ModifierFlags::COMMAND)),
        in_app(vec![])
    );
}

#[test]
fn the_ls_are_chromes_alone() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(m.handle(&key(Key::KeyL)), in_app(vec![]));
    assert_eq!(
        m.handle(&key_with(Key::KeyL, ModifierFlags::SHIFT)),
        in_app(vec![])
    );
}

fn site_showing(url: &str) -> Mercury {
    let mut m = chrome_showing(url);
    let _ = m.handle(&key(Key::KeyS));
    m
}

#[test]
fn claude_ai_n_starts_a_new_chat_and_enters_typing() {
    let mut m = site_showing("https://claude.ai/new");
    assert_eq!(
        m.handle(&key(Key::KeyN)),
        vec![
            tap(Key::KeyO, ModifierFlags::COMMAND | ModifierFlags::SHIFT),
            shows("Typing")
        ]
    );
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn n_is_claude_ais_alone() {
    let mut m = site_showing("https://www.x.com/asdfasdf");
    assert_eq!(m.handle(&key(Key::KeyN)), vec![return_home_timer()]);
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Site(_))));
}

#[test]
fn inapp_s_enters_site() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(
        m.handle(&key(Key::KeyS)),
        vec![shows("Site"), return_home_timer()]
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Site(_))));
}

#[test]
fn inapp_n_enters_nav() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(
        m.handle(&key(Key::KeyN)),
        vec![shows("Nav"), return_home_timer()]
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))));
}

#[test]
fn inapp_t_enters_typing() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(m.handle(&key(Key::KeyT)), vec![shows("Typing")]);
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn inapp_app_bindings_still_take_precedence() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(
        m.handle(&key(Key::KeyJ)),
        in_app(tmux(ModifierFlags::empty(), Key::KeyP))
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
}

#[test]
fn chrome_r_refreshes() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(cmd_r()));
}

#[test]
fn inapp_other_app_ignores_keys() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Zed, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert!(matches!(front(&m), Some(App::Zed | App::Other)));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(vec![]));
}

#[test]
fn unbound_key_is_none() {
    let mut m = home();
    assert_eq!(m.handle(&key(Key::KeyX)), vec![]);
}

// If the prefix were held through the command, tmux would see `ctrl-p` rather than `p`.
fn tmux(flags: ModifierFlags, command: Key) -> Vec<MercuryEffect> {
    vec![tap(Key::KeyA, ModifierFlags::CONTROL), tap(command, flags)]
}

#[test]
fn i_enters_ghostty_in_app_when_ghostty_is_frontmost() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Ghostty));
}

#[test]
fn ghostty_j_is_previous_window_and_k_is_next() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));

    assert_eq!(
        m.handle(&key(Key::KeyJ)),
        in_app(tmux(ModifierFlags::empty(), Key::KeyP))
    );
    assert_eq!(
        m.handle(&key(Key::KeyK)),
        in_app(tmux(ModifierFlags::empty(), Key::KeyN))
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Ghostty));
}

#[test]
fn the_tmux_command_is_a_bare_tap() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    let effects = m.handle(&key(Key::KeyJ));

    assert_eq!(effects.len(), 3);
    assert_eq!(effects[0], tap(Key::KeyA, ModifierFlags::CONTROL));
    assert_eq!(effects[1], tap(Key::KeyP, ModifierFlags::empty()));
    assert_eq!(effects[2], return_home_timer());
}

#[test]
fn j_and_k_are_unbound_in_chrome_in_app() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(m.handle(&key(Key::KeyJ)), in_app(vec![]));
    assert_eq!(m.handle(&key(Key::KeyK)), in_app(vec![]));
}

#[test]
fn foregrounding_ghostty_retargets_the_inapp_layer() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Chrome));

    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Ghostty));
    assert_eq!(
        m.handle(&key(Key::KeyJ)),
        in_app(tmux(ModifierFlags::empty(), Key::KeyP))
    );
}

// The tmux config binds `!`..`)` to windows 1..10; bare digits cannot reach the tenth.
#[test]
fn the_digits_select_a_tmux_window_and_return_home() {
    for (k, expected) in [
        (Key::Num1, Key::Num1),
        (Key::Num5, Key::Num5),
        (Key::Num9, Key::Num9),
        (Key::Num0, Key::Num0),
    ] {
        let mut m = home();
        let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
        let _ = m.handle(&key(Key::KeyI));

        assert_eq!(
            m.handle(&key(k)),
            leaves(tmux(ModifierFlags::SHIFT, expected)),
            "{k:?}"
        );
        assert!(
            matches!(m.layer(), Layer::Home(_)),
            "{k:?} stayed in ghostty"
        );
    }
}

#[test]
fn all_ten_digits_are_bound_in_ghostty() {
    let digits = [
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
        Key::Num0,
    ];
    for digit in digits {
        let mut m = home();
        let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
        let _ = m.handle(&key(Key::KeyI));
        assert_eq!(
            m.handle(&key(digit)),
            leaves(tmux(ModifierFlags::SHIFT, digit)),
            "{digit:?} is unbound"
        );
    }
}

#[test]
fn walking_stays_but_jumping_leaves() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));

    let _ = m.handle(&key(Key::KeyJ));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Ghostty));
    let _ = m.handle(&key(Key::Num3));
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn inapp_activity_resets_the_return_home_timer() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Ghostty, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(
        m.handle(&key(Key::KeyJ)),
        in_app(tmux(ModifierFlags::empty(), Key::KeyP))
    );
    assert_eq!(
        m.handle(&key(Key::Num3)),
        leaves(tmux(ModifierFlags::SHIFT, Key::Num3))
    );
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn the_digits_are_unbound_outside_ghostty() {
    let mut m = home();
    assert_eq!(m.handle(&key(Key::Num1)), vec![]);

    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(m.handle(&key(Key::Num1)), in_app(vec![]));
}

#[test]
fn home_r_enters_resize() {
    let mut m = home();
    assert_eq!(
        m.handle(&key(Key::KeyR)),
        vec![shows("Resize"), return_home_timer()]
    );
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Resize(_))));
}

#[test]
fn the_arrows_place_the_window_and_return_home() {
    for (k, frame) in [
        (Key::UpArrow, SCREEN.visible),
        (
            Key::LeftArrow,
            Frame {
                width: 800.0,
                ..SCREEN.visible
            },
        ),
        (
            Key::RightArrow,
            Frame {
                x: 800.0,
                width: 800.0,
                ..SCREEN.visible
            },
        ),
    ] {
        let mut m = home_with_a_window();
        let _ = m.handle(&key(Key::KeyR));
        assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Resize(_))));

        assert_eq!(
            m.handle(&key(k)),
            leaves(vec![
                MercuryEffect::SetFrame(Placement {
                    window: WINDOW,
                    from: WINDOW_FRAME,
                    to: frame,
                }),
                settle_timer(),
            ]),
            "{k:?}"
        );
        assert!(
            matches!(m.layer(), Layer::Home(_)),
            "{k:?} stayed in resize"
        );
    }
}

#[test]
fn a_placement_with_no_focused_window_asks_for_nothing() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(m.handle(&key(Key::UpArrow)), leaves(vec![]));
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn escape_leaves_resize() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyR));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Resize(_))));

    // The rearm precedes; go_home drops its guard, so the timer is cancelled.
    assert_eq!(
        m.handle(&key(Key::Escape)),
        vec![return_home_timer(), shows("Home")]
    );
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn placing_twice_re_enters_resize() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(
        m.handle(&key(Key::UpArrow)),
        leaves(vec![
            MercuryEffect::SetFrame(Placement {
                window: WINDOW,
                from: WINDOW_FRAME,
                to: SCREEN.visible,
            }),
            settle_timer(),
        ])
    );
    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(
        m.handle(&key(Key::LeftArrow)),
        leaves(vec![
            MercuryEffect::SetFrame(Placement {
                window: WINDOW,
                from: WINDOW_FRAME,
                to: Frame {
                    width: 800.0,
                    ..SCREEN.visible
                },
            }),
            settle_timer(),
        ])
    );
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn the_arrows_are_unbound_in_home() {
    let mut m = home();
    assert_eq!(m.handle(&key(Key::UpArrow)), vec![]);
    assert_eq!(m.handle(&key(Key::LeftArrow)), vec![]);
    assert_eq!(m.handle(&key(Key::RightArrow)), vec![]);
}

#[test]
fn r_still_refreshes_chrome_in_app() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(cmd_r()));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Chrome));
}

fn settle(
    runner: &mut SimpleRunner<'_, MercuryStruct, Mercury>,
    performed: &mut Vec<MercuryEffect>,
) {
    while let Some(effects) = runner.next() {
        for effect in effects {
            if let MercuryEffect::Foreground(app) = &effect {
                runner.queue_event(foreground(*app, Pid(7)));
            }
            performed.push(effect);
        }
    }
}

#[test]
fn foregrounding_chrome_is_reported_back() {
    let mut m = home();
    let mut performed = Vec::new();
    {
        let mut runner = SimpleRunner::<MercuryStruct, _>::new(&mut m);
        for k in [Key::KeyN, Key::KeyC] {
            runner.queue_event(key(k));
            settle(&mut runner, &mut performed);
        }
    }
    assert_eq!(
        performed,
        vec![
            shows("Nav"),
            return_home_timer(),
            MercuryEffect::Foreground(App::Chrome),
            shows("App"),
            return_home_timer(),
        ]
    );
    assert_eq!(front(&m), Some(App::Chrome));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert!(front(&m).is_some());
}

#[test]
fn bundle_id_round_trips() {
    for app in [
        App::Chrome,
        App::Codex,
        App::Discord,
        App::Ghostty,
        App::Obsidian,
        App::Zed,
    ] {
        let id = app.bundle_id().expect("a real app has a bundle id");
        assert_eq!(App::from_bundle_id(id), app);
    }
    assert_eq!(App::Other.bundle_id(), None);
    assert_eq!(App::from_bundle_id("com.example.Unknown"), App::Other);
}

#[test]
fn reported_bundle_ids_map() {
    assert_eq!(App::from_bundle_id("com.google.Chrome"), App::Chrome);
    assert_eq!(App::from_bundle_id("com.openai.codex"), App::Codex);
    assert_eq!(App::from_bundle_id("com.hnc.Discord"), App::Discord);
    assert_eq!(App::from_bundle_id("com.mitchellh.ghostty"), App::Ghostty);
    assert_eq!(App::from_bundle_id("md.obsidian"), App::Obsidian);
    assert_eq!(App::from_bundle_id("dev.zed.Zed"), App::Zed);
    assert_eq!(App::from_bundle_id("Google Chrome"), App::Other);
}

#[test]
fn the_inapp_layers_bindings_follow_the_root_with_no_resync() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyI));
    m.foreground = Some(FrontApp::new(App::Chrome, Pid(7)));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(cmd_r()));

    m.foreground = Some(FrontApp::new(App::Ghostty, Pid(7)));

    assert_eq!(m.handle(&key(Key::KeyR)), in_app(vec![]));
    assert_eq!(
        m.handle(&key(Key::KeyJ)),
        in_app(tmux(ModifierFlags::empty(), Key::KeyP))
    );

    m.foreground = Some(FrontApp::new(App::Zed, Pid(7)));
    assert_eq!(m.handle(&key(Key::KeyJ)), in_app(vec![]));
}

#[test]
fn foreground_retargets_the_inapp_layer() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Chrome));

    assert_eq!(m.handle(&foreground(App::Zed, Pid(7))), vec![]);
    assert_eq!(front(&m), Some(App::Zed));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert!(matches!(front(&m), Some(App::Zed | App::Other)));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(vec![]));
}

#[test]
fn foreground_back_to_chrome_restores_its_bindings() {
    let mut m = home();
    let _ = m.handle(&foreground(App::Zed, Pid(7)));
    let _ = m.handle(&key(Key::KeyI));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert!(matches!(front(&m), Some(App::Zed | App::Other)));

    let _ = m.handle(&foreground(App::Chrome, Pid(7)));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Chrome));
    assert_eq!(m.handle(&key(Key::KeyR)), in_app(cmd_r()));
}

#[test]
fn foreground_outside_inapp_does_not_change_layer() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyN));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))));

    assert_eq!(m.handle(&foreground(App::Chrome, Pid(7))), vec![]);
    assert_eq!(front(&m), Some(App::Chrome));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Nav(_))));
}

#[test]
fn inapp_follows_the_front_app_across_a_switch() {
    let mut m = home();
    let mut performed = Vec::new();
    {
        let mut runner = SimpleRunner::<MercuryStruct, _>::new(&mut m);
        for k in [Key::KeyN, Key::KeyC] {
            runner.queue_event(key(k));
            settle(&mut runner, &mut performed);
        }
    }
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert_eq!(front(&m), Some(App::Chrome));
    let _ = m.handle(&foreground(App::Zed, Pid(7)));
    assert_eq!(front(&m), Some(App::Zed));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::InApp(_))));
    assert!(matches!(front(&m), Some(App::Zed | App::Other)));
}

fn jk_timer() -> MercuryEffect {
    let (_guard, effect) = freddie::timer_effect_and_guard(JK_TIMEOUT, ());
    MercuryEffect::Timer(effect)
}

fn typing() -> Mercury {
    Mercury::new(Some(FrontApp::new(App::Other, Pid(1))), Windows::default())
}

fn jk(m: &Mercury) -> &freddie::KeySequence {
    match m.layer() {
        Layer::Typing(t) => &t.jk,
        other => panic!("not in typing: {other:?}"),
    }
}

#[test]
fn jk_typed_one_key_at_a_time_leaves_for_home() {
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert!(!jk(&m).is_idle());
    assert_eq!(m.handle(&up(Key::KeyJ)), vec![]);
    assert_eq!(m.handle(&key(Key::KeyK)), vec![shows("Home")]);
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn jk_rolled_leaves_for_home_and_the_ups_land_in_home() {
    // k goes down before j comes up. The two ups arrive in Home, which is not passthrough, so they are swallowed rather than reaching the app as ups with no downs.
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(m.handle(&key(Key::KeyK)), vec![shows("Home")]);
    assert!(matches!(m.layer(), Layer::Home(_)));
    assert_eq!(m.handle(&up(Key::KeyJ)), vec![]);
    assert_eq!(m.handle(&up(Key::KeyK)), vec![]);
}

#[test]
fn a_j_tap_then_another_key_types_the_j_first() {
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(m.handle(&up(Key::KeyJ)), vec![]);
    assert_eq!(
        m.handle(&key(Key::KeyA)),
        vec![
            emit(Key::KeyJ, PressType::Down),
            emit(Key::KeyJ, PressType::Up),
            emit(Key::KeyA, PressType::Down),
        ]
    );
}

#[test]
fn a_held_j_then_another_key_replays_only_its_down() {
    // Only the j down was swallowed, so only it replays. The real j up passes through later.
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(
        m.handle(&key(Key::KeyA)),
        vec![
            emit(Key::KeyJ, PressType::Down),
            emit(Key::KeyA, PressType::Down),
        ]
    );
    assert_eq!(
        m.handle(&up(Key::KeyJ)),
        vec![emit(Key::KeyJ, PressType::Up)]
    );
}

#[test]
fn a_j_carrying_a_modifier_never_opens_the_run() {
    let mut m = typing();
    assert_eq!(
        m.handle(&key_with(Key::KeyJ, ModifierFlags::COMMAND)),
        vec![emit_with(
            Key::KeyJ,
            PressType::Down,
            ModifierFlags::COMMAND
        )]
    );
    assert!(jk(&m).is_idle());
}

#[test]
fn a_modifier_arriving_mid_run_breaks_it() {
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(
        m.handle(&key_with(Key::MetaLeft, ModifierFlags::COMMAND)),
        vec![
            emit(Key::KeyJ, PressType::Down),
            emit_with(Key::MetaLeft, PressType::Down, ModifierFlags::COMMAND),
        ]
    );
    assert!(jk(&m).is_idle());
}

#[test]
fn a_held_js_auto_repeat_breaks_the_run() {
    // The swallowed down replays ahead of the repeat, so the app sees the same two downs it would have seen unwatched.
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(
        m.handle(&key(Key::KeyJ)),
        vec![
            emit(Key::KeyJ, PressType::Down),
            emit(Key::KeyJ, PressType::Down),
        ]
    );
    assert!(jk(&m).is_idle());
    assert_eq!(m.handle(&key(Key::KeyK)), passed(Key::KeyK));
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn escape_in_typing_breaks_the_run_and_reaches_the_app() {
    // Escape runs through the sequence like any other key; the j replays ahead of it.
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(m.handle(&up(Key::KeyJ)), vec![]);
    assert_eq!(
        m.handle(&key(Key::Escape)),
        vec![
            emit(Key::KeyJ, PressType::Down),
            emit(Key::KeyJ, PressType::Up),
            emit(Key::Escape, PressType::Down),
        ]
    );
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn leaving_typing_abandons_a_held_j() {
    // The layer change drops the run; the j is dropped rather than typed, and its up will be swallowed by the command layer.
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(m.handle(&key(Key::KeyK)), vec![shows("Home")]);
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn j_and_k_still_type_themselves_when_they_are_not_a_run() {
    // The second j breaks the first run and does not open a second, so all three type.
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(m.handle(&up(Key::KeyJ)), vec![]);
    assert_eq!(
        m.handle(&key(Key::KeyJ)),
        vec![
            emit(Key::KeyJ, PressType::Down),
            emit(Key::KeyJ, PressType::Up),
            emit(Key::KeyJ, PressType::Down),
        ]
    );
    assert_eq!(
        m.handle(&up(Key::KeyJ)),
        vec![emit(Key::KeyJ, PressType::Up)]
    );
    assert_eq!(m.handle(&key(Key::KeyK)), passed(Key::KeyK));
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn a_half_typed_run_types_itself_when_the_window_elapses() {
    let mut m = typing();
    let opened = m.handle(&key(Key::KeyJ));
    assert_eq!(opened, vec![jk_timer()]);
    assert_eq!(
        m.handle(&timer_event(&opened)),
        vec![emit(Key::KeyJ, PressType::Down)]
    );
    assert!(jk(&m).is_idle());
    assert_eq!(m.handle(&key(Key::KeyK)), passed(Key::KeyK));
    assert!(matches!(m.layer(), Layer::Typing(_)));
}

#[test]
fn a_full_tap_types_itself_when_the_window_elapses() {
    let mut m = typing();
    let opened = m.handle(&key(Key::KeyJ));
    assert_eq!(opened, vec![jk_timer()]);
    assert_eq!(m.handle(&up(Key::KeyJ)), vec![]);
    assert_eq!(
        m.handle(&timer_event(&opened)),
        vec![
            emit(Key::KeyJ, PressType::Down),
            emit(Key::KeyJ, PressType::Up),
        ]
    );
}

#[test]
fn a_firing_from_a_run_that_ended_matches_nothing() {
    // First window's firing arrives after a second run replaced it. It must not interrupt the live one.
    let mut m = typing();
    let first = timer_event(&m.handle(&key(Key::KeyJ)));
    let _ = m.handle(&key(Key::KeyA));
    let second = timer_event(&m.handle(&key(Key::KeyJ)));

    assert_eq!(
        m.handle(&first),
        vec![],
        "no binding matches a stale firing"
    );
    assert!(!jk(&m).is_idle(), "the live run is untouched");

    assert_eq!(m.handle(&second), vec![emit(Key::KeyJ, PressType::Down)]);
}

#[test]
fn a_firing_with_no_run_in_progress_matches_nothing() {
    let mut m = typing();
    let stale = timer_event(&m.handle(&key(Key::KeyJ)));
    let _ = m.handle(&key(Key::KeyA));
    assert_eq!(m.handle(&stale), vec![]);
}

#[test]
fn the_window_is_armed_once_per_run_not_once_per_key() {
    // The j up advances the run without re-arming: the window runs from the first key.
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyJ)), vec![jk_timer()]);
    assert_eq!(m.handle(&up(Key::KeyJ)), vec![]);
}

fn overlay_hide_timer() -> MercuryEffect {
    let (_guard, effect) = freddie::timer_effect_and_guard(OVERLAY_DWELL, ());
    MercuryEffect::Timer(effect)
}

// In a timed layer the deadline post rearms below the root, so that effect precedes the overlay's.
fn shown_at(effects: &[MercuryEffect]) -> usize {
    effects
        .iter()
        .position(|e| matches!(e, MercuryEffect::ShowOverlay(_)))
        .unwrap_or_else(|| panic!("o shows the overlay: {effects:?}"))
}

// Asserted by heading so re-wording a row does not rewrite the test table.
fn shown_heading(effects: &[MercuryEffect]) -> &'static str {
    let at = shown_at(effects);
    match &effects[at..] {
        [
            MercuryEffect::ShowOverlay(text),
            MercuryEffect::Timer(_),
            ..,
        ] => text.lines().next().expect("a keymap has a heading"),
        other => panic!("o shows the overlay and sets its hide: {other:?}"),
    }
}

fn dwell_event(effects: &[MercuryEffect]) -> MercuryEvent {
    timer_event(&effects[shown_at(effects)..])
}

#[test]
fn o_shows_the_layers_keymap() {
    for (enter, heading) in [
        (None, "  HOME"),
        (Some(Key::KeyN), "  NAV"),
        (Some(Key::KeyR), "  RESIZE"),
    ] {
        let mut m = home();
        if let Some(k) = enter {
            let _ = m.handle(&key(k));
        }
        let effects = m.handle(&key(Key::KeyO));
        assert_eq!(shown_heading(&effects), heading);
        assert_eq!(effects[shown_at(&effects) + 1], overlay_hide_timer());
    }
}

#[test]
fn showing_the_overlay_rearms_the_return_home_timer() {
    for enter in [Key::KeyN, Key::KeyR] {
        let mut m = home();
        let _ = m.handle(&key(enter));
        let effects = m.handle(&key(Key::KeyO));
        assert_eq!(
            effects.len(),
            3,
            "the rearm, then the overlay and its dwell"
        );
        assert_eq!(effects[0], return_home_timer());
        assert_eq!(effects[2], overlay_hide_timer());
    }

    let mut m = home();
    let effects = m.handle(&key(Key::KeyO));
    assert_eq!(effects.len(), 2, "home has no return-home timer to rearm");
    assert_eq!(effects[1], overlay_hide_timer());
}

#[test]
fn the_in_app_keymap_is_the_front_apps() {
    for (app, heading) in [
        (App::Chrome, "  CHROME"),
        (App::Ghostty, "  GHOSTTY"),
        (App::Zed, "  IN-APP"),
    ] {
        let mut m = home();
        let _ = m.handle(&foreground(app, Pid(7)));
        let _ = m.handle(&key(Key::KeyI));
        let effects = m.handle(&key(Key::KeyO));
        assert_eq!(shown_heading(&effects), heading, "{app:?}");
    }
}

#[test]
fn the_overlay_hides_after_the_dwell() {
    let mut m = home();
    let shown = m.handle(&key(Key::KeyO));
    assert_eq!(
        m.handle(&dwell_event(&shown)),
        vec![MercuryEffect::HideOverlay]
    );
    assert_eq!(m.handle(&dwell_event(&shown)), vec![]);
}

#[test]
fn o_again_takes_it_down() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyO));
    assert_eq!(m.handle(&key(Key::KeyO)), vec![MercuryEffect::HideOverlay]);
    let effects = m.handle(&key(Key::KeyO));
    assert_eq!(shown_heading(&effects), "  HOME");
}

#[test]
fn a_dwell_from_a_showing_already_gone_matches_nothing() {
    // First showing's dwell arrives after nav replaced it. It must not take the live one down.
    let mut m = home();
    let first = dwell_event(&m.handle(&key(Key::KeyO)));
    let _ = m.handle(&key(Key::KeyN));
    let second = dwell_event(&m.handle(&key(Key::KeyO)));

    assert_eq!(m.handle(&first), vec![]);
    assert_eq!(m.handle(&second), vec![MercuryEffect::HideOverlay]);
}

#[test]
fn changing_layers_takes_the_overlay_down() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyO));
    assert_eq!(
        m.handle(&key(Key::KeyN)),
        vec![
            MercuryEffect::HideOverlay,
            shows("Nav"),
            return_home_timer(),
        ]
    );
}

#[test]
fn a_transition_with_no_overlay_hides_nothing() {
    let mut m = home();
    assert_eq!(
        m.handle(&key(Key::KeyN)),
        vec![shows("Nav"), return_home_timer()]
    );
}

#[test]
fn o_in_typing_is_typed() {
    let mut m = typing();
    assert_eq!(m.handle(&key(Key::KeyO)), passed(Key::KeyO));
}

const SCREEN: Monitor = Monitor {
    full: Frame {
        x: 0.0,
        y: 0.0,
        width: 1600.0,
        height: 925.0,
    },
    visible: Frame {
        x: 0.0,
        y: 25.0,
        width: 1600.0,
        height: 900.0,
    },
};
const WINDOW: WindowId = WindowId(7);
const WINDOW_FRAME: Frame = Frame {
    x: 100.0,
    y: 100.0,
    width: 400.0,
    height: 300.0,
};

const fn windows(change: WindowChange) -> MercuryEvent {
    MercuryEvent::Window(WindowEvent { change })
}

// Land every read the fact requested, carrying the riding half home.
fn land_frames(m: &mut Mercury, fx: Vec<MercuryEffect>, frame: Option<Frame>) {
    for effect in fx {
        if let MercuryEffect::ReadFrame { window, generation } = effect {
            let _ = m.handle(&frame_read(window, generation, frame));
        }
    }
}

fn window_at(m: &mut Mercury, change: WindowChange, frame: Frame) {
    let fx = m.handle(&windows(change));
    land_frames(m, fx, Some(frame));
}

fn focus_lands(m: &mut Mercury, pid: Pid, window: Option<WindowId>) {
    let fx = m.handle(&windows(WindowChange::FocusChanged(pid)));
    for effect in fx {
        if let MercuryEffect::ReadFocus { pid, generation } = effect {
            let _ = m.handle(&focus_read(pid, generation, window));
        }
    }
}

fn home_with_a_window() -> Mercury {
    let mut m = home();
    let _ = m.handle(&windows(WindowChange::Screens(vec![SCREEN])));
    window_at(&mut m, WindowChange::Opened(WINDOW), WINDOW_FRAME);
    focus_lands(&mut m, Pid(1), Some(WINDOW));
    m
}

#[test]
fn an_opened_window_is_recorded_with_its_frame() {
    let m = home_with_a_window();
    assert_eq!(m.windows.focused(Pid(1)), Some((WINDOW, WINDOW_FRAME)));
}

#[test]
fn a_window_fact_requests_its_read() {
    let mut m = home();
    let fx = m.handle(&windows(WindowChange::Opened(WINDOW)));
    assert_eq!(fx.len(), 1);
    assert!(matches!(
        fx[0],
        MercuryEffect::ReadFrame { window: WINDOW, .. }
    ));
    assert_eq!(
        m.handle(&windows(WindowChange::Moved(WindowId(999)))),
        vec![]
    );
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn a_stale_frame_read_is_dropped() {
    let mut m = home_with_a_window();
    let first = m.handle(&windows(WindowChange::Moved(WINDOW)));
    let second = m.handle(&windows(WindowChange::Moved(WINDOW)));
    land_frames(&mut m, first, Some(SCREEN.visible));
    assert_eq!(m.windows.focused(Pid(1)), None);
    land_frames(&mut m, second, Some(SCREEN.visible));
    assert_eq!(m.windows.focused(Pid(1)), Some((WINDOW, SCREEN.visible)));
}

#[test]
fn a_move_and_a_resize_both_replace_the_frame() {
    for change in [WindowChange::Moved(WINDOW), WindowChange::Resized(WINDOW)] {
        let mut m = home_with_a_window();
        window_at(&mut m, change, SCREEN.visible);
        assert_eq!(m.windows.focused(Pid(1)), Some((WINDOW, SCREEN.visible)));
    }
}

#[test]
fn a_closed_window_leaves_no_frame_and_no_focus() {
    let mut m = home_with_a_window();
    let _ = m.handle(&windows(WindowChange::Closed(WINDOW)));
    assert_eq!(m.windows.focused(Pid(1)), None);
}

#[test]
fn focus_on_an_unknown_window_yields_nothing_focused() {
    let mut m = home_with_a_window();
    focus_lands(&mut m, Pid(1), Some(WindowId(999)));
    assert_eq!(m.windows.focused(Pid(1)), None);
}

#[test]
fn a_background_pids_focus_is_stored_not_dropped() {
    let mut m = home_with_a_window();
    let other = WindowId(8);
    window_at(&mut m, WindowChange::Opened(other), SCREEN.visible);
    focus_lands(&mut m, Pid(2), Some(other));
    assert_eq!(m.windows.focused(Pid(1)), Some((WINDOW, WINDOW_FRAME)));
    assert_eq!(m.windows.focused(Pid(2)), Some((other, SCREEN.visible)));
}

// A change during boot arrives in the install burst and again as an event.
#[test]
fn recording_a_change_twice_is_recording_it_once() {
    let mut once = home_with_a_window();
    let mut twice = home_with_a_window();
    window_at(&mut twice, WindowChange::Opened(WINDOW), WINDOW_FRAME);
    assert_eq!(once.windows.focused(Pid(1)), twice.windows.focused(Pid(1)));

    focus_lands(&mut once, Pid(1), Some(WINDOW));
    focus_lands(&mut twice, Pid(1), Some(WINDOW));
    focus_lands(&mut twice, Pid(1), Some(WINDOW));
    assert_eq!(once.windows.focused(Pid(1)), twice.windows.focused(Pid(1)));
}

#[test]
fn the_monitor_is_the_one_the_window_is_on() {
    let m = home_with_a_window();
    assert_eq!(m.windows.monitor_for(WINDOW_FRAME), Some(SCREEN));

    let off = Frame {
        x: 9000.0,
        ..WINDOW_FRAME
    };
    assert_eq!(m.windows.monitor_for(off), Some(SCREEN));
}

#[test]
fn no_screens_reported_means_no_monitor() {
    let m = home();
    assert_eq!(m.windows.monitor_for(WINDOW_FRAME), None);
}

#[test]
fn a_placement_uses_the_screen_the_window_is_on() {
    const SECOND: Monitor = Monitor {
        full: Frame {
            x: 1600.0,
            y: 0.0,
            width: 1000.0,
            height: 800.0,
        },
        visible: Frame {
            x: 1600.0,
            y: 25.0,
            width: 1000.0,
            height: 775.0,
        },
    };
    let on_second = Frame {
        x: 1700.0,
        ..WINDOW_FRAME
    };

    let mut m = home();
    let _ = m.handle(&windows(WindowChange::Screens(vec![SCREEN, SECOND])));
    window_at(&mut m, WindowChange::Opened(WINDOW), on_second);
    focus_lands(&mut m, Pid(1), Some(WINDOW));
    let _ = m.handle(&key(Key::KeyR));

    assert_eq!(
        m.handle(&key(Key::UpArrow)),
        leaves(vec![
            MercuryEffect::SetFrame(Placement {
                window: WINDOW,
                from: on_second,
                to: SECOND.visible,
            }),
            settle_timer(),
        ])
    );
}

#[test]
fn resize_r_restores_the_frame_from_before_the_placement() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::UpArrow));
    window_at(&mut m, WindowChange::Moved(WINDOW), SCREEN.visible);

    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(
        m.handle(&key(Key::KeyR)),
        leaves(vec![
            MercuryEffect::SetFrame(Placement {
                window: WINDOW,
                from: SCREEN.visible,
                to: WINDOW_FRAME,
            }),
            settle_timer(),
        ])
    );
    assert!(matches!(m.layer(), Layer::Home(_)));
}

#[test]
fn a_second_placement_does_not_move_the_remembered_frame() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::UpArrow));
    window_at(&mut m, WindowChange::Moved(WINDOW), SCREEN.visible);
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::LeftArrow));

    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(
        m.handle(&key(Key::KeyR)),
        leaves(vec![
            MercuryEffect::SetFrame(Placement {
                window: WINDOW,
                from: SCREEN.visible,
                to: WINDOW_FRAME,
            }),
            settle_timer(),
        ])
    );
}

#[test]
fn the_intermediate_frames_of_a_placement_are_not_a_move_by_hand() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::UpArrow));

    for frame in [
        Frame {
            x: 0.0,
            y: 25.0,
            ..WINDOW_FRAME
        },
        SCREEN.visible,
    ] {
        window_at(&mut m, WindowChange::Moved(WINDOW), frame);
    }

    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(
        m.handle(&key(Key::KeyR)),
        leaves(vec![
            MercuryEffect::SetFrame(Placement {
                window: WINDOW,
                from: SCREEN.visible,
                to: WINDOW_FRAME,
            }),
            settle_timer(),
        ])
    );
}

// `set_frame` writes position and size twice; ending the wait on the first report would treat the rest as a drag.
#[test]
fn the_target_frame_reported_twice_is_still_not_a_move_by_hand() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::UpArrow));

    for _ in 0..2 {
        window_at(&mut m, WindowChange::Moved(WINDOW), SCREEN.visible);
        window_at(&mut m, WindowChange::Resized(WINDOW), SCREEN.visible);
    }

    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(
        m.handle(&key(Key::KeyR)),
        leaves(vec![
            MercuryEffect::SetFrame(Placement {
                window: WINDOW,
                from: SCREEN.visible,
                to: WINDOW_FRAME,
            }),
            settle_timer(),
        ])
    );
}

#[test]
fn a_move_by_hand_forgets_the_remembered_frame() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    let effects = m.handle(&key(Key::UpArrow));
    let _ = m.handle(&timer_event(&effects));

    window_at(
        &mut m,
        WindowChange::Moved(WINDOW),
        Frame {
            x: 700.0,
            ..WINDOW_FRAME
        },
    );

    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(m.handle(&key(Key::KeyR)), leaves(vec![]));
}

#[test]
fn restoring_twice_asks_for_nothing_the_second_time() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::UpArrow));
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::KeyR));

    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(m.handle(&key(Key::KeyR)), leaves(vec![]));
}

#[test]
fn a_pending_frame_places_nothing() {
    let mut m = home_with_a_window();
    let _ = m.handle(&windows(WindowChange::Moved(WINDOW)));
    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(m.handle(&key(Key::UpArrow)), leaves(vec![]));
}

#[test]
fn r_in_resize_does_not_re_enter_resize() {
    let mut m = home();
    let _ = m.handle(&key(Key::KeyR));
    assert!(matches!(return_home(&m), Some(ReturnHomeLayers::Resize(_))));
    let _ = m.handle(&key(Key::KeyR));
    assert!(matches!(m.layer(), Layer::Home(_)));
}

// A reused `CGWindowID` must not restore a new window to a closed one's frame.
#[test]
fn a_closed_window_is_forgotten() {
    let mut m = home_with_a_window();
    let _ = m.handle(&key(Key::KeyR));
    let _ = m.handle(&key(Key::UpArrow));

    let _ = m.handle(&windows(WindowChange::Closed(WINDOW)));
    window_at(&mut m, WindowChange::Opened(WINDOW), SCREEN.visible);
    focus_lands(&mut m, Pid(1), Some(WINDOW));

    let _ = m.handle(&key(Key::KeyR));
    assert_eq!(m.handle(&key(Key::KeyR)), leaves(vec![]));
}
