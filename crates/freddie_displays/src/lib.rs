//! Active displays, and a watcher that reports the full current set on every
//! screen-parameter change and every wake from sleep.
//!
//! Always the whole set, never a delta. Wake can change what is lit without a
//! screen-parameter notification. [`displays`] and [`watch`]'s callback run on the
//! main thread. macOS only.

use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use core_graphics::display::CGDisplay;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSApplicationDidChangeScreenParametersNotification, NSScreen, NSWorkspace,
    NSWorkspaceDidWakeNotification,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSNotificationName, NSNumber, NSString,
};

pub use freddie_displays_types::{Display, DisplayId};

/// The displays currently active. For seeding; [`watch`] reports changes.
///
/// # Panics
///
/// Off the main thread.
#[must_use]
pub fn displays() -> Vec<Display> {
    let mtm = MainThreadMarker::new().expect("displays() must run on the main thread");
    read(mtm)
}

fn read(mtm: MainThreadMarker) -> Vec<Display> {
    NSScreen::screens(mtm)
        .iter()
        .filter_map(|screen| {
            // `NSScreenNumber` is the CGDirectDisplayID. Skip a screen without one.
            let number = screen
                .deviceDescription()
                .objectForKey(&*NSString::from_str("NSScreenNumber"))?
                .downcast::<NSNumber>()
                .ok()?
                .as_u32();
            Some(Display {
                id: DisplayId(number),
                builtin: CGDisplay::new(number).is_builtin(),
                name: screen.localizedName().to_string(),
            })
        })
        .collect()
}

/// Call `on_change` with the full current set on every screen-parameter change and every wake.
/// Delivery is on the main thread. Dropping the [`Watcher`] deregisters both observers.
#[must_use = "dropping the watcher deregisters the observers; hold it to keep receiving events"]
pub fn watch<F>(on_change: F) -> Watcher
where
    F: Fn(Vec<Display>) + Send + 'static,
{
    let on_change = Arc::new(on_change);
    // SAFETY: both notification names are immutable extern statics AppKit initializes before
    // any notification can be delivered.
    #[expect(unsafe_code)]
    let (screen_name, wake_name) = unsafe {
        (
            NSApplicationDidChangeScreenParametersNotification,
            NSWorkspaceDidWakeNotification,
        )
    };
    let screens = observe(
        &NSNotificationCenter::defaultCenter(),
        screen_name,
        Arc::clone(&on_change),
    );
    let wake = observe(
        &NSWorkspace::sharedWorkspace().notificationCenter(),
        wake_name,
        on_change,
    );
    Watcher {
        _screens: screens,
        _wake: wake,
    }
}

fn observe<F>(
    center: &Retained<NSNotificationCenter>,
    name: &'static NSNotificationName,
    on_change: Arc<F>,
) -> Observation
where
    F: Fn(Vec<Display>) + Send + 'static,
{
    let block = RcBlock::new(move |_notif: NonNull<NSNotification>| {
        // Skip rather than panic: this is an FFI frame.
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let displays = read(mtm);
        tracing::debug!(?displays, "display topology reported");
        on_change(displays);
    });
    // SAFETY: the block is `Send` because `F` is. `Observation` removes the observer before
    // either is dropped.
    #[expect(unsafe_code)]
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(name),
            None, // any sender
            None, // no queue: deliver on the posting thread, which is main
            &block,
        )
    };
    Observation {
        center: center.clone(),
        token,
        _block: block,
    }
}

/// Screen-parameter and wake observers. Dropping it deregisters both.
#[must_use = "dropping the watcher deregisters the observers"]
pub struct Watcher {
    _screens: Observation,
    _wake: Observation,
}

struct Observation {
    center: Retained<NSNotificationCenter>,
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    /// The center copies the block; the closure it wraps is ours to keep alive.
    _block: RcBlock<dyn Fn(NonNull<NSNotification>)>,
}

impl Drop for Observation {
    /// Dropping the token alone would leave the center calling a block whose closure is gone.
    fn drop(&mut self) {
        let observer: &AnyObject = (*self.token).as_ref();
        // SAFETY: `token` is what `addObserverForName…` returned and it is still registered,
        // so this is the documented way to deregister it.
        #[expect(unsafe_code)]
        unsafe {
            self.center.removeObserver(observer);
        }
    }
}
