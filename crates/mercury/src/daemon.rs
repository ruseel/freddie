//! Grab the keyboard and drive the model.
//!
//! Main thread: `AppKit` run loop. Tap thread: `CGEventTap`, spawned by `intercept`. Worker: tokio, owns state and `Emitter`, runs event and effect loops.
//!
//! Quit is a `Kill` effect that ends the effect loop so destructors release the keyboard and stop the run loop.

use std::ops::ControlFlow;
use std::time::Instant;

use freddie::TimerEffect;
use freddie_keyboard::Emitter;
use freddie_overlay::OverlaySink;
use freddie_windows::WindowSink;
use mercury::{
    App, Chord, FrontApp, Mercury, MercuryEffect, MercuryEvent, WindowEvent, Windows, focus_read,
    foreground, frame_read, quit_event,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot::error::TryRecvError;
use tracing::{debug, error, info, warn};

use crate::automation_socket::{self, AutomationCommand};

pub(crate) enum RuntimeEffect {
    Mercury(MercuryEffect),
    Automation(AutomationCommand),
}

/// Give the main thread to the `AppKit` run loop; run mercury on a worker thread.
///
/// `AppKit` delivers callbacks only while main is in a run loop. Dropping the worker's `Stopper` stops that loop. A panic aborts from the panic hook instead. Declaration order: the runtime drops before the `Stopper`.
pub(crate) fn run(port: u16, automation_port: u16) {
    // Accessory NSApp, before the status item and before the loop pumps events.
    freddie_main_loop::init_menu_bar_app();

    let (main_loop, stopper, waker) = freddie_main_loop::main_loop();

    // Created here: the menu bar's Quit handler runs on this thread and needs a sender.
    let (event_tx, event_rx) = unbounded_channel::<MercuryEvent>();

    // Status-item titles. NSStatusItem is main-thread-only. A waking std channel, not tokio's, because the receiver is the main thread.
    let (title_tx, title_rx) = waker.channel::<&'static str>();

    // Quit click enqueues the same event as any source. Mouse-reachable even if the keyboard is wedged.
    let menu_bar = freddie_menu_bar::show(
        "Mercury",
        freddie_menu_bar::IconKind::Template(include_bytes!("../assets/mercury.png")),
        {
            let event_tx = event_tx.clone();
            move || {
                let _ = event_tx.send(quit_event());
            }
        },
    );
    let menu_bar = match menu_bar {
        Ok(bar) => bar,
        Err(e) => {
            error!(error = %e, "could not create the menu bar");
            return;
        }
    };
    // Title is known before the model exists, so paint it now. Leading space is the gap after the glyph.
    menu_bar.set_title(Some(&format!(" {}", Mercury::BOOT_TITLE)));

    // NSPanel is main-thread-only. Held for the run; dropping it closes the panel.
    let (overlay, overlay_sink) = freddie_overlay::overlay(&waker);

    // Installed before the seed is read so a switch in between is queued rather than lost. Dropping it deregisters.
    let _app_watcher = freddie_app_nav::watch({
        let event_tx = event_tx.clone();
        move |front| {
            let _ = event_tx.send(foreground(App::from_bundle_id(&front.bundle_id), front.pid));
        }
    });

    // Watcher is `!Send`; observers register on this thread's run loop. The install pass queues the seed burst as events.
    let windows = freddie_windows::watch(&waker, {
        let event_tx = event_tx.clone();
        move |change| {
            let _ = event_tx.send(MercuryEvent::Window(WindowEvent { change }));
        }
    });
    let (window_watcher, window_sink) = match windows {
        Ok(watcher) => {
            let sink = watcher.sink();
            (Some(watcher), Some(sink))
        }
        Err(e) => {
            error!(error = %e, "window observation unavailable");
            (None, None)
        }
    };

    let boot = Boot {
        front: freddie_app_nav::frontmost()
            .map(|front| FrontApp::new(App::from_bundle_id(&front.bundle_id), front.pid)),
        window_sink,
        overlay: overlay_sink,
    };

    let worker = std::thread::Builder::new()
        .name("mercury-runtime".to_owned())
        .spawn(move || {
            let _stopper = stopper; // dropped last: runtime must drop first
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a current-thread runtime with no reactor cannot fail to build");
            runtime.block_on(serve(
                boot,
                event_tx,
                event_rx,
                title_tx,
                port,
                automation_port,
            ));
        })
        .expect("spawning the runtime thread");

    // Last title only: intermediate layers in one batch are not worth showing. Leading space is the gap after the glyph. Overlay messages apply in order so Hide after Show in the same wake still hides.
    main_loop.run(|| {
        if let Some(name) = title_rx.try_iter().last() {
            menu_bar.set_title(Some(&format!(" {name}")));
        }
        overlay.pump();
        if let Some(watcher) = window_watcher.as_ref() {
            watcher.pump();
        }
    });
    let _ = worker.join();
    // Keep the icon and panel up until the loop returns.
    drop(menu_bar);
    drop(overlay);
}

/// What the process read from the OS before the main loop started.
///
/// OS reads are allowed while this is built and at no point after: once `main_loop.run` is going, every fact reaches the model as an event.
struct Boot {
    /// Already-frontmost app. `None` when nothing is frontmost. The watcher reports later changes.
    front: Option<FrontApp>,
    /// Placement handle. `None` when window observation could not start.
    window_sink: Option<WindowSink>,
    overlay: OverlaySink,
}

/// Worker-thread mercury. `!Send` because `Emitter`'s `CGEventSource` mutates on post and must stay on this thread.
#[expect(clippy::future_not_send)]
async fn serve(
    boot: Boot,
    event_tx: UnboundedSender<MercuryEvent>,
    event_rx: UnboundedReceiver<MercuryEvent>,
    title_tx: freddie_main_loop::WakingSender<&'static str>,
    port: u16,
    automation_port: u16,
) {
    let (effect_tx, effect_rx) = unbounded_channel::<RuntimeEffect>();

    // Dropping it closes the port. Above the keyboard grab so a refused start has not taken the keyboard. A busy port panics: the single-instance lock means the squatter is some other program.
    let _socket = freddie_event_socket::listen(port, {
        let event_tx = event_tx.clone();
        move |text| mercury::on_message(text, &event_tx)
    })
    .unwrap_or_else(|e| {
        panic!("could not bind 127.0.0.1:{port}: {e}; find it with `lsof -i :{port}`")
    });

    let _automation_socket = automation_socket::listen(automation_port, effect_tx.clone())
        .await
        .unwrap_or_else(|e| {
            panic!(
                "could not bind 127.0.0.1:{automation_port}: {e}; find it with `lsof -i :{automation_port}`"
            )
        });

    let grabbed = freddie_keyboard::intercept({
        let event_tx = event_tx.clone();
        move |ev| {
            // Swallow every key. Dropping an up leaves a modifier stuck: a swallowed ctrl-up makes the next key still carry ctrl.
            let _ = event_tx.send(MercuryEvent::Key(ev));
            None
        }
    });
    let (interceptor, emitter) = match grabbed {
        Ok(pair) => pair,
        Err(e) => {
            // Usually Accessibility is not granted.
            error!(error = %e, "could not intercept the keyboard");
            return;
        }
    };

    let mouse_grabbed = freddie_keyboard::intercept_mouse(emitter.tag(), {
        let event_tx = event_tx.clone();
        move |ev| {
            let _ = event_tx.send(MercuryEvent::MouseButton(ev));
            None
        }
    });
    let mouse_interceptor = match mouse_grabbed {
        Ok(interceptor) => Some(interceptor),
        Err(e) => {
            warn!(error = %e, "could not intercept mouse side buttons");
            None
        }
    };

    // SIGTERM from `launchctl bootout` and `mercury stop` becomes the same Quit as the menu bar, so the keyboard is released.
    // Spawned, not a third `select!` arm: a completed arm would drop the other futures and skip the graceful path.
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut term) => {
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                if term.recv().await.is_some() {
                    info!("SIGTERM: quitting");
                    let _ = event_tx.send(quit_event());
                }
            });
        }
        Err(e) => {
            warn!(error = %e, "no SIGTERM handler; a terminated mercury will not release the keyboard");
        }
    }

    // `select!` rather than `join!`: the effect loop ends on `Kill`; the event loop never does, because the tap thread holds a sender for as long as the grab is alive.
    let mercury = Mercury::new(boot.front, Windows::default());

    // No initial title: `run` already painted `Mercury::BOOT_TITLE`. The worker sends only changes.
    tokio::select! {
        () = run_event_loop(mercury, event_rx, effect_tx) => {}
        () = run_effect_loop(effect_rx, emitter, event_tx, title_tx, boot.window_sink, boot.overlay) => {}
    }
    drop(mouse_interceptor);
    drop(interceptor); // hold the grab until here
}

async fn run_event_loop(
    mut state: Mercury,
    mut event_rx: UnboundedReceiver<MercuryEvent>,
    effect_tx: UnboundedSender<RuntimeEffect>,
) {
    info!(state = ?state, "initial state");
    while let Some(event) = event_rx.recv().await {
        dispatch_event(&mut state, &event, &effect_tx);
    }
}

/// One log record per dispatch: event, effects, resulting state.
fn dispatch_event(
    state: &mut Mercury,
    event: &MercuryEvent,
    effect_tx: &UnboundedSender<RuntimeEffect>,
) {
    let start = Instant::now();
    let effects = state.handle(event);
    let duration_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
    info!(event = ?event, effects = ?effects, duration_us, state = ?state, "dispatch");
    for effect in effects {
        let _ = effect_tx.send(RuntimeEffect::Mercury(effect));
    }
}

/// Perform effects in dispatch order until one says to stop. `!Send` for the same reason as [`serve`].
#[expect(clippy::future_not_send)]
async fn run_effect_loop(
    mut effect_rx: UnboundedReceiver<RuntimeEffect>,
    emitter: Emitter,
    event_tx: UnboundedSender<MercuryEvent>,
    title_tx: freddie_main_loop::WakingSender<&'static str>,
    windows: Option<WindowSink>,
    overlay: OverlaySink,
) {
    while let Some(effect) = effect_rx.recv().await {
        let flow = match effect {
            RuntimeEffect::Mercury(effect) => perform_effect(
                effect,
                &emitter,
                &event_tx,
                &title_tx,
                windows.as_ref(),
                &overlay,
            ),
            RuntimeEffect::Automation(command) => perform_automation(command, &emitter),
        };
        if flow.is_break() {
            break;
        }
    }
}

fn perform_automation(command: AutomationCommand, emitter: &Emitter) -> ControlFlow<()> {
    match command {
        AutomationCommand::Tap(chord) => match emitter.tap(chord.key, chord.flags) {
            Ok(()) => debug!(key = ?chord.key, flags = ?chord.flags, "automation tapped"),
            Err(e) => {
                warn!(key = ?chord.key, flags = ?chord.flags, error = %e, "automation tap failed")
            }
        },
        AutomationCommand::Emit(event) => match emitter.emit(event.key, event.press, event.flags) {
            Ok(()) => debug!(key = ?event.key, press = ?event.press, "automation emitted"),
            Err(e) => {
                warn!(key = ?event.key, press = ?event.press, error = %e, "automation emit failed")
            }
        },
        AutomationCommand::Foreground(bundle_id) => foreground_bundle(bundle_id),
    }
    ControlFlow::Continue(())
}

/// `Kill` breaks rather than exiting so destructors release the keyboard and stop the run loop.
fn perform_effect(
    effect: MercuryEffect,
    emitter: &Emitter,
    event_tx: &UnboundedSender<MercuryEvent>,
    title_tx: &freddie_main_loop::WakingSender<&'static str>,
    windows: Option<&WindowSink>,
    overlay: &OverlaySink,
) -> ControlFlow<()> {
    match effect {
        MercuryEffect::Foreground(app) => foreground_app(app),
        MercuryEffect::Tap(Chord { key, flags }) => match emitter.tap(key, flags) {
            Ok(()) => debug!(?key, ?flags, "tapped"),
            Err(e) => warn!(?key, ?flags, error = %e, "tap failed"),
        },
        MercuryEffect::Emit(ke) => match emitter.emit(ke.key, ke.press, ke.flags) {
            Ok(()) => debug!(key = ?ke.key, press = ?ke.press, "emitted"),
            Err(e) => warn!(key = ?ke.key, press = ?ke.press, error = %e, "emit failed"),
        },
        MercuryEffect::MouseButtonTap(button) => match emitter.tap_mouse(button) {
            Ok(()) => debug!(?button, "mouse button tapped"),
            Err(e) => warn!(?button, error = %e, "mouse button tap failed"),
        },
        MercuryEffect::SetFrame(placement) => {
            if let Some(windows) = windows {
                if let Err(e) = windows.set_frame(placement) {
                    warn!(?placement, error = %e, "set frame failed");
                }
            } else {
                debug!(?placement, "no window sink: nothing to place through");
            }
        }
        MercuryEffect::ReadFrame { window, generation } => {
            let event_tx = event_tx.clone();
            std::thread::spawn(move || {
                let frame = freddie_windows::frame_of(window);
                let _ = event_tx.send(frame_read(window, generation, frame));
            });
        }
        MercuryEffect::ReadFocus { pid, generation } => {
            let event_tx = event_tx.clone();
            std::thread::spawn(move || {
                let window = freddie_windows::focused_window_of(pid);
                let _ = event_tx.send(focus_read(pid, generation, window));
            });
        }
        MercuryEffect::Copy(what) => copy(what),
        MercuryEffect::Kill => {
            info!("kill: exiting");
            return ControlFlow::Break(());
        }
        MercuryEffect::Timer(timer) => schedule_timer(timer, event_tx),
        MercuryEffect::ShowOverlay(text) => overlay.show(text.to_owned()),
        MercuryEffect::HideOverlay => overlay.hide(),
        // Main thread is gone; Kill handles that.
        MercuryEffect::ShowLayer(name) => {
            let _ = title_tx.send(name);
        }
    }
    ControlFlow::Continue(())
}

/// Fire the timer's event after its delay unless the guard drops first. Spawned so the sleep is off the effect loop.
fn schedule_timer(timer: TimerEffect, event_tx: &UnboundedSender<MercuryEvent>) {
    let TimerEffect {
        delay,
        event,
        mut cancel,
    } = timer;
    // Guard already dropped: cancelled, nothing to spawn.
    if matches!(cancel.try_recv(), Err(TryRecvError::Closed)) {
        return;
    }
    let event_tx = event_tx.clone();
    tokio::spawn(async move {
        tokio::select! {
            () = tokio::time::sleep(delay) => { let _ = event_tx.send(MercuryEvent::Timer(event)); }
            _ = cancel => {}
        }
    });
}

/// Copy on its own thread: `arboard` talks to `NSPasteboard`. The pasteboard keeps the text after `Clipboard` drops.
fn copy(text: String) {
    std::thread::spawn(move || {
        match arboard::Clipboard::new().and_then(|mut board| board.set_text(text.clone())) {
            Ok(()) => debug!(%text, "copied"),
            Err(e) => warn!(%text, error = %e, "copy failed"),
        }
    });
}

/// Foreground on its own thread so the effect loop never blocks on `open`. The watcher reports what actually came up.
fn foreground_app(app: App) {
    let Some(bundle_id) = app.bundle_id() else {
        warn!(app = ?app, "no bundle id; not foregrounding");
        return;
    };
    std::thread::spawn(move || match freddie_app_nav::foreground(bundle_id) {
        Ok(()) => debug!(app = bundle_id, "foregrounded"),
        Err(e) => warn!(app = bundle_id, error = %e, "foreground failed"),
    });
}

fn foreground_bundle(bundle_id: String) {
    std::thread::spawn(move || match freddie_app_nav::foreground(&bundle_id) {
        Ok(()) => debug!(app = %bundle_id, "automation foregrounded"),
        Err(e) => warn!(app = %bundle_id, error = %e, "automation foreground failed"),
    });
}
