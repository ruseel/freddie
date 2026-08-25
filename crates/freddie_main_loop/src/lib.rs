//! Give the main thread to the platform run loop.
//!
//! [`MainLoop::run`] pumps `NSApplication` events. A bare `CFRunLoop` never
//! dispatches the window-server `NSEvent`s that a status item's clicks and menu
//! tracking need. Call [`init_menu_bar_app`] once on the main thread before creating
//! a status item. The loop sleeps in `nextEventMatchingMask` with no deadline until
//! a real event or a posted wake. Dropping the [`Stopper`] wakes it to exit.
//!
//! ```no_run
//! let (main_loop, stopper, _waker) = freddie_main_loop::main_loop();
//!
//! std::thread::spawn(move || {
//!     let _stopper = stopper; // dropping it stops main
//!     // ... all of the program's work, on this thread ...
//! });
//!
//! main_loop.run(|| {}); // returns once the worker drops the stopper
//! ```
//!
//! macOS only.

use std::ptr::NonNull;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use core_foundation::base::TCFType;
use core_foundation::runloop::CFRunLoop;
use objc2::MainThreadMarker;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask, NSEventModifierFlags,
    NSEventSubtype, NSEventType,
};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSPoint};

/// Subtype on events posted only to wake the loop, so they are not dispatched as real events.
const WAKE_SUBTYPE: i16 = 1;

/// Initialize `NSApplication` as an accessory (menu-bar) app.
///
/// Call once, on the main thread, before creating any status item. Accessory policy
/// keeps the process out of the Dock and the cmd-tab switcher. `finishLaunching` is
/// required because [`MainLoop::run`] pumps the loop itself rather than calling `[NSApp run]`.
///
/// # Panics
///
/// If called off the main thread.
pub fn init_menu_bar_app() {
    let mtm = MainThreadMarker::new().expect("init_menu_bar_app must be called on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
}

/// Create the main loop, the handle that stops it, and the waker that wakes it.
/// Call on the main thread, before spawning the worker that takes the [`Stopper`].
///
/// # Panics
///
/// If called off the main thread.
pub fn main_loop() -> (MainLoop, Stopper, MainWaker) {
    let mtm = MainThreadMarker::new().expect("main_loop must be called on the main thread");
    let waker = MainWaker {
        app_handle: AppHandle::new(&NSApplication::sharedApplication(mtm)),
    };
    let (stop_signal, stop_receiver) = std::sync::mpsc::channel();
    (
        MainLoop { stop_receiver },
        Stopper {
            stop_signal,
            waker: waker.clone(),
        },
        waker,
    )
}

/// Handle to the process `NSApplication`, for posting a wake event from any thread.
/// `NSApp` outlives the process; `postEvent:atStart:` is thread-safe.
#[derive(Clone)]
struct AppHandle(NonNull<NSApplication>);

// SAFETY: `NSApp` lives for the whole process and `postEvent:atStart:` is thread-safe, so the
// pointer stays valid and the call is sound from any thread.
#[expect(unsafe_code)]
unsafe impl Send for AppHandle {}
#[expect(unsafe_code)]
unsafe impl Sync for AppHandle {}

impl AppHandle {
    fn new(app: &NSApplication) -> Self {
        Self(NonNull::from(app))
    }

    fn post_wake_event(&self) {
        #[expect(unsafe_code)]
        // SAFETY: `self.0` is the process `NSApp`, valid for the process; `postEvent:atStart:` is
        // thread-safe; the event is a fresh application-defined event carrying `WAKE_SUBTYPE`.
        unsafe {
            let app = self.0.as_ref();
            let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                NSEventType::ApplicationDefined,
                NSPoint::new(0.0, 0.0),
                NSEventModifierFlags::empty(),
                0.0,
                0,
                None,
                WAKE_SUBTYPE,
                0,
                0,
            );
            if let Some(event) = event {
                app.postEvent_atStart(&event, true);
            }
        }
    }
}

/// Wakes the main loop from any thread so work queued for `on_wake` runs now.
#[derive(Clone)]
pub struct MainWaker {
    app_handle: AppHandle,
}

impl MainWaker {
    /// Wake the main loop. Send on the channel first: the send has to be visible when
    /// `on_wake` drains.
    pub fn wake(&self) {
        self.app_handle.post_wake_event();
    }

    /// A channel whose sender wakes the loop on every send.
    #[must_use]
    pub fn channel<T>(&self) -> (WakingSender<T>, Receiver<T>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        (
            WakingSender {
                sender,
                waker: self.clone(),
            },
            receiver,
        )
    }
}

/// A channel sender that wakes the main loop after each send.
pub struct WakingSender<T> {
    sender: Sender<T>,
    waker: MainWaker,
}

// Hand-written: `#[derive(Clone)]` would add a spurious `T: Clone` bound.
impl<T> Clone for WakingSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl<T> WakingSender<T> {
    /// Send, then wake the main loop.
    ///
    /// # Errors
    ///
    /// If the receiver has been dropped.
    pub fn send(&self, value: T) -> Result<(), std::sync::mpsc::SendError<T>> {
        self.sender.send(value)?;
        self.waker.wake();
        Ok(())
    }
}

/// The main thread's run loop.
#[must_use = "the main loop does nothing until it is run"]
pub struct MainLoop {
    stop_receiver: Receiver<()>,
}

impl MainLoop {
    /// Run until the [`Stopper`] is dropped. `on_wake` runs on the main thread after
    /// each real event or posted wake, and must return promptly.
    ///
    /// # Panics
    ///
    /// If called off the main thread.
    pub fn run(self, mut on_wake: impl FnMut()) {
        assert!(
            is_main_thread(),
            "MainLoop::run must be called on the main thread: AppKit delivers only there"
        );
        let mtm = MainThreadMarker::new().expect("run is on the main thread; asserted above");
        let app = NSApplication::sharedApplication(mtm);
        while matches!(self.stop_receiver.try_recv(), Err(TryRecvError::Empty)) {
            autoreleasepool(|_| {
                #[expect(unsafe_code)]
                // SAFETY: on the main thread; dequeuing one event, waiting indefinitely.
                let event = unsafe {
                    app.nextEventMatchingMask_untilDate_inMode_dequeue(
                        NSEventMask::Any,
                        Some(&NSDate::distantFuture()),
                        NSDefaultRunLoopMode,
                        true,
                    )
                };
                if let Some(event) = event {
                    // A wake event exists only to break the wait; do not dispatch it.
                    if !is_wake_event(&event) {
                        app.sendEvent(&event);
                    }
                }
                on_wake();
            });
        }
    }
}

/// Stops the main loop when dropped, from any thread.
pub struct Stopper {
    stop_signal: Sender<()>,
    waker: MainWaker,
}

impl Drop for Stopper {
    fn drop(&mut self) {
        // Signal first, then wake: the loop must see the stop when the posted event
        // breaks its wait. A post that lands before the loop blocks sits in the event queue.
        let _ = self.stop_signal.send(());
        self.waker.wake();
    }
}

fn is_main_thread() -> bool {
    CFRunLoop::get_current().as_concrete_TypeRef() == CFRunLoop::get_main().as_concrete_TypeRef()
}

fn is_wake_event(event: &NSEvent) -> bool {
    event.r#type() == NSEventType::ApplicationDefined
        && event.subtype() == NSEventSubtype(WAKE_SUBTYPE)
}

#[cfg(test)]
mod tests {
    use super::{MainLoop, Stopper, is_main_thread};
    use std::sync::mpsc::{TryRecvError, channel};

    #[test]
    fn the_test_thread_is_not_main() {
        assert!(!is_main_thread());
    }

    #[test]
    fn run_off_the_main_thread_panics() {
        let (_stop_signal, stop_receiver) = channel::<()>();
        let main_loop = MainLoop { stop_receiver };
        let panicked =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| main_loop.run(|| {})));
        assert!(panicked.is_err(), "run() off main must panic, not hang");
    }

    #[test]
    fn a_stop_signal_reaches_the_loop() {
        let (stop_signal, stop_receiver) = channel::<()>();
        let main_loop = MainLoop { stop_receiver };
        assert!(matches!(
            main_loop.stop_receiver.try_recv(),
            Err(TryRecvError::Empty)
        ));
        let _ = stop_signal.send(());
        assert!(main_loop.stop_receiver.try_recv().is_ok());
    }

    #[test]
    fn the_stopper_is_send() {
        const fn assert_send<T: Send>() {}
        assert_send::<Stopper>();
    }
}
