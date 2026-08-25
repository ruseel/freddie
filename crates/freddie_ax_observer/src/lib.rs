//! One `AXObserver` per observable app, kept current as apps launch and quit.
//!
//! [`watch_apps`] creates an observer for every observable app now running and for every
//! one that launches later. The consumer brings the C callback, the registration builder,
//! and the per-app install and teardown hooks. Every callback runs on the main thread.
//! macOS only.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use accessibility_sys::{
    AXObserverAddNotification, AXObserverCreate, AXObserverGetRunLoopSource, AXObserverRef,
    AXUIElementCreateApplication, AXUIElementRef,
};
use block2::RcBlock;
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::runloop::{CFRunLoop, CFRunLoopSource, kCFRunLoopDefaultMode};
use core_foundation::string::{CFString, CFStringRef};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidLaunchApplicationNotification, NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNotificationName};

pub use freddie_windows_types::Pid;

/// An app whose windows a user could be looking at.
///
/// macOS UI services (`CursorUIViewService`, `Open and Save Panel Service`) own real
/// windows and post the same Accessibility notifications. Observing them records an
/// invisible 64x64 box as the focused window. Their activation policy is `Prohibited`.
/// Accessory apps stay in: a menu-bar app has windows.
#[derive(Clone, Copy, Debug)]
pub struct ObservableApp(pub Pid);

impl ObservableApp {
    /// `app` if its windows can be looked at, `None` if it is a UI service.
    #[must_use]
    pub fn of(app: &NSRunningApplication) -> Option<Self> {
        (app.activationPolicy() != NSApplicationActivationPolicy::Prohibited)
            .then(|| Self(Pid(app.processIdentifier())))
    }
}

/// One app the watcher can see, borrowed for the duration of one callback.
pub struct AppSeen {
    pub pid: Pid,
    pub observer: AXObserverRef,
    pub app_element: AXUIElementRef,
}

/// The C notification callback a consumer brings: what `AXObserverCreate` takes.
pub type NotificationCallback =
    unsafe extern "C" fn(AXObserverRef, AXUIElementRef, CFStringRef, *mut c_void);

type OnApp = Box<dyn Fn(&AppSeen, *mut c_void)>;

/// One app's observer, and the `refcon` its callbacks reach the consumer's state through.
struct AppObserver<R> {
    observer: AXObserverRef,
    /// Boxed so its address is stable; freed when the observer naming it is.
    _registration: Box<R>,
}

impl<R> Drop for AppObserver<R> {
    /// Remove the run loop source, then release the observer. The source must be gone
    /// before the registration that its callbacks dereference is dropped.
    fn drop(&mut self) {
        // SAFETY: `observer` is live and was created by `AXObserverCreate`. Getting its
        // source takes no ownership; removing it and releasing the observer is the
        // documented teardown.
        #[expect(unsafe_code)]
        unsafe {
            let source = AXObserverGetRunLoopSource(self.observer);
            CFRunLoop::get_main().remove_source(
                &CFRunLoopSource::wrap_under_get_rule(source),
                kCFRunLoopDefaultMode,
            );
            CFRelease(self.observer.cast());
        }
    }
}

/// Per-app map and consumer hooks. Main-thread only.
struct Inner<R> {
    apps: RefCell<HashMap<Pid, AppObserver<R>>>,
    callback: NotificationCallback,
    make_registration: Box<dyn Fn(&AppSeen) -> R>,
    on_app: OnApp,
    on_app_gone: Box<dyn Fn(Pid)>,
}

/// One `AXObserver` per observable app. `R` is the consumer's per-app registration,
/// boxed so its address is stable for the life of that app's observer. `!Send`.
pub struct AppWatch<R> {
    /// Declared first so they stop before the map they write into is torn down.
    _notifications: Vec<Observation>,
    _inner: Rc<Inner<R>>,
}

/// Create one app's observer and hand the consumer its hooks. An app that refuses
/// Accessibility, or has not finished launching, is logged at `debug` and skipped.
fn observe_app<R>(inner: &Rc<Inner<R>>, ObservableApp(pid): ObservableApp) {
    if inner.apps.borrow().contains_key(&pid) {
        return;
    }

    // Before the observer, so an early return between the two `Create` calls has nothing to release.
    // SAFETY: `pid` names a live process and the element is +1, released with the `Owned`.
    #[expect(unsafe_code)]
    let app = unsafe { AXUIElementCreateApplication(pid.0) };
    let Some(app) = Owned::new(app.cast()) else {
        return;
    };
    let app_element: AXUIElementRef = app.0.cast_mut().cast();

    let mut observer: AXObserverRef = std::ptr::null_mut();
    // SAFETY: `pid` names a process; the out-parameter receives a +1 observer on success
    // and is untouched otherwise.
    #[expect(unsafe_code)]
    let status = unsafe { AXObserverCreate(pid.0, inner.callback, &raw mut observer) };
    if status != 0 || observer.is_null() {
        tracing::debug!(?pid, status, "could not observe an app");
        return;
    }

    let seen = AppSeen {
        pid,
        observer,
        app_element,
    };
    let registration = Box::new((inner.make_registration)(&seen));
    let refcon = std::ptr::from_ref(registration.as_ref()).cast_mut().cast();

    // SAFETY: `observer` is live; its source is owned by the observer and added at +0.
    #[expect(unsafe_code)]
    unsafe {
        let source = AXObserverGetRunLoopSource(observer);
        CFRunLoop::get_main().add_source(
            &CFRunLoopSource::wrap_under_get_rule(source),
            kCFRunLoopDefaultMode,
        );
    }

    inner.apps.borrow_mut().insert(
        pid,
        AppObserver {
            observer,
            _registration: registration,
        },
    );

    // After the insert, so a consumer hook that fires a notification synchronously finds
    // the app already observed.
    (inner.on_app)(&seen, refcon);
}

/// Drop the observer before `on_app_gone`, so a late notification cannot run against a
/// registration that no longer exists.
fn forget_app<R>(inner: &Inner<R>, pid: Pid) {
    if inner.apps.borrow_mut().remove(&pid).is_none() {
        return;
    }
    (inner.on_app_gone)(pid);
}

/// Observe every running observable app now and every one that launches later.
///
/// `callback` is the consumer's C notification callback. `on_app` runs once per observed
/// app and receives the stable `refcon`. `on_app_gone` runs after the observer is torn down.
pub fn watch_apps<R: 'static>(
    callback: NotificationCallback,
    make_registration: impl Fn(&AppSeen) -> R + 'static,
    on_app: impl Fn(&AppSeen, *mut c_void) + 'static,
    on_app_gone: impl Fn(Pid) + 'static,
) -> AppWatch<R> {
    let inner = Rc::new(Inner {
        apps: RefCell::new(HashMap::new()),
        callback,
        make_registration: Box::new(make_registration),
        on_app: Box::new(on_app),
        on_app_gone: Box::new(on_app_gone),
    });

    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    let mut notifications = Vec::new();
    for (name, launched) in [
        // SAFETY: both are immutable extern statics AppKit initializes at startup.
        #[expect(unsafe_code)]
        (unsafe { NSWorkspaceDidLaunchApplicationNotification }, true),
        #[expect(unsafe_code)]
        (
            unsafe { NSWorkspaceDidTerminateApplicationNotification },
            false,
        ),
    ] {
        let inner = Rc::downgrade(&inner);
        notifications.push(observe_notification(&workspace, name, move |notif| {
            let (Some(inner), Some(app)) = (inner.upgrade(), notified_app(notif)) else {
                return;
            };
            if launched {
                if let Some(app) = ObservableApp::of(&app) {
                    observe_app(&inner, app);
                }
            } else {
                forget_app(&inner, Pid(app.processIdentifier()));
            }
        }));
    }

    for app in NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter_map(|app| ObservableApp::of(&app))
    {
        observe_app(&inner, app);
    }

    tracing::debug!(apps = inner.apps.borrow().len(), "observing apps");
    AppWatch {
        _notifications: notifications,
        _inner: inner,
    }
}

/// Subscribe `observer` to one notification on `element`, carrying `refcon`.
/// A failure is logged and skipped.
///
/// # Safety
///
/// `observer` and `element` must be live, and `refcon` must stay valid for as long as the
/// observer can deliver.
#[expect(unsafe_code)]
pub unsafe fn add_notification(
    observer: AXObserverRef,
    element: AXUIElementRef,
    notification: &str,
    refcon: *mut c_void,
) {
    let name = CFString::new(notification);
    // SAFETY: the caller's contract, plus `name` living for the call.
    let status =
        unsafe { AXObserverAddNotification(observer, element, name.as_concrete_TypeRef(), refcon) };
    if status != 0 {
        tracing::debug!(notification, status, "could not add a notification");
    }
}

/// A +1 CoreFoundation reference, released when it drops. Not `Copy` or `Clone`.
struct Owned(CFTypeRef);

impl Owned {
    fn new(raw: CFTypeRef) -> Option<Self> {
        (!raw.is_null()).then_some(Self(raw))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: an `Owned` is only built from a +1 reference, and only here is it
        // released, once.
        #[expect(unsafe_code)]
        unsafe {
            CFRelease(self.0);
        }
    }
}

/// One registered notification observer, deregistered when it drops.
/// The center is held with the token because deregistering needs the same one that registered.
pub struct Observation {
    center: Retained<NSNotificationCenter>,
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    /// The center copies the block; the closure it wraps is ours to keep alive.
    _block: RcBlock<dyn Fn(NonNull<NSNotification>)>,
}

impl Drop for Observation {
    fn drop(&mut self) {
        let observer: &AnyObject = (*self.token).as_ref();
        // SAFETY: `token` is what `addObserverForName...` returned on `center` and is still
        // registered, so this is the documented way to deregister it.
        #[expect(unsafe_code)]
        unsafe {
            self.center.removeObserver(observer);
        }
    }
}

pub fn observe_notification(
    center: &Retained<NSNotificationCenter>,
    name: &NSNotificationName,
    on_notification: impl Fn(&NSNotification) + 'static,
) -> Observation {
    let block = RcBlock::new(move |notif: NonNull<NSNotification>| {
        // SAFETY: Foundation hands the block a valid notification, live for this call.
        #[expect(unsafe_code)]
        let notif = unsafe { notif.as_ref() };
        on_notification(notif);
    });

    // SAFETY: `name` is an immutable extern static. The block runs on the main thread.
    // `Observation` deregisters before the token or the block is dropped.
    #[expect(unsafe_code)]
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
    };

    Observation {
        center: center.clone(),
        token,
        _block: block,
    }
}

/// The app a launch or terminate notification is about.
#[must_use]
pub fn notified_app(notif: &NSNotification) -> Option<Retained<NSRunningApplication>> {
    let info = notif.userInfo()?;
    // SAFETY: `NSWorkspaceApplicationKey` is an immutable extern static `NSString` that
    // AppKit initializes before any notification can be delivered.
    #[expect(unsafe_code)]
    let key = unsafe { NSWorkspaceApplicationKey };
    info.objectForKey(key)?
        .downcast::<NSRunningApplication>()
        .ok()
}
