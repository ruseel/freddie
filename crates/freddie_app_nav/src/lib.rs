//! Bring an app to the front, and watch which app is frontmost, by bundle identifier.
//!
//! [`foreground`] asks the OS. [`watch`] reports `NSWorkspace`'s
//! `didActivateApplication` notification. Delivery is on the main thread;
//! `on_change` must hand its work elsewhere and return. macOS only.

use std::fmt;
use std::process::Command;
use std::ptr::NonNull;

use block2::RcBlock;
use freddie_windows_types::Pid;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::NSNotification;

/// Foregrounding an app failed.
#[derive(Debug)]
pub enum NavError {
    /// `open` could not be spawned at all.
    Spawn(std::io::Error),
    /// `open` ran but reported failure (the app is missing, or activation was
    /// refused).
    Failed,
}

impl fmt::Display for NavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not run `open`: {e}"),
            Self::Failed => {
                f.write_str("`open` reported failure (app missing or activation refused)")
            }
        }
    }
}

impl std::error::Error for NavError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(e) => Some(e),
            Self::Failed => None,
        }
    }
}

/// Bring the app with this bundle identifier to the front, launching it if needed.
/// Does not confirm the app came up; [`watch`] reports the real frontmost app.
///
/// # Errors
///
/// [`NavError::Spawn`] if `open` cannot be spawned, [`NavError::Failed`] if it
/// exits non-zero.
pub fn foreground(bundle_id: &str) -> Result<(), NavError> {
    let status = Command::new("open")
        .args(open_args(bundle_id))
        .status()
        .map_err(NavError::Spawn)?;
    if status.success() {
        Ok(())
    } else {
        Err(NavError::Failed)
    }
}

const fn open_args(bundle_id: &str) -> [&str; 2] {
    ["-b", bundle_id]
}

/// The frontmost app, or `None`. For seeding. Polling this returns the app that
/// was frontmost at process start; use [`watch`] for changes.
#[must_use]
pub fn frontmost() -> Option<FrontmostApp> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(FrontmostApp {
        bundle_id: app.bundleIdentifier()?.to_string(),
        pid: Pid(app.processIdentifier()),
    })
}

/// The frontmost app as macOS reports it. An app with no bundle identifier is not reported.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FrontmostApp {
    pub bundle_id: String,
    pub pid: Pid,
}

fn activated_app(notif: &NSNotification) -> Option<FrontmostApp> {
    let info = notif.userInfo()?;
    // SAFETY: `NSWorkspaceApplicationKey` is an immutable extern static `NSString`
    // that AppKit initializes before any notification can be delivered.
    #[expect(unsafe_code)]
    let key = unsafe { NSWorkspaceApplicationKey };
    let app = info
        .objectForKey(key)?
        .downcast::<NSRunningApplication>()
        .ok()?;
    Some(FrontmostApp {
        bundle_id: app.bundleIdentifier()?.to_string(),
        pid: Pid(app.processIdentifier()),
    })
}

/// Call `on_change` for each app as it becomes frontmost. Seed with [`frontmost`].
/// The callback runs on the main thread. Dropping the [`Watcher`] deregisters.
#[must_use = "dropping the watcher deregisters the observer; hold it to keep receiving events"]
pub fn watch<F>(on_change: F) -> Watcher
where
    F: Fn(&FrontmostApp) + Send + 'static,
{
    let block = RcBlock::new(move |notif: NonNull<NSNotification>| {
        // SAFETY: Foundation hands the block a valid, retained notification, live
        // for the duration of this call.
        #[expect(unsafe_code)]
        let notif = unsafe { notif.as_ref() };
        if let Some(front) = activated_app(notif) {
            tracing::debug!(app = %front.bundle_id, pid = front.pid.0, "frontmost app changed");
            on_change(&front);
        }
    });

    // SAFETY: the notification name is an immutable extern static. The block is
    // `Send` because `F` is. `Watcher` removes the observer before either is dropped.
    #[expect(unsafe_code)]
    let token = unsafe {
        NSWorkspace::sharedWorkspace()
            .notificationCenter()
            .addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceDidActivateApplicationNotification),
                None, // any sender
                None, // no queue: deliver on the posting thread, which is main
                &block,
            )
    };

    Watcher {
        token,
        _block: block,
    }
}

/// A live `didActivateApplication` observer. Dropping it deregisters.
#[must_use = "dropping the watcher deregisters the observer"]
pub struct Watcher {
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    /// The center copies the block; the closure it wraps is ours to keep alive.
    _block: RcBlock<dyn Fn(NonNull<NSNotification>)>,
}

impl Drop for Watcher {
    /// `removeObserver` is required: dropping the token alone leaves the center
    /// calling the block after the closure is gone.
    fn drop(&mut self) {
        let observer: &AnyObject = (*self.token).as_ref();
        // SAFETY: `token` is what `addObserverForName...` returned and it is still
        // registered, so this is the documented way to deregister it.
        #[expect(unsafe_code)]
        unsafe {
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .removeObserver(observer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{frontmost, open_args};

    #[test]
    fn open_args_are_dash_b_bundle_id() {
        assert_eq!(open_args("com.google.Chrome"), ["-b", "com.google.Chrome"]);
        assert_eq!(open_args("dev.zed.Zed"), ["-b", "dev.zed.Zed"]);
    }

    #[test]
    fn frontmost_is_a_bundle_id_or_nothing() {
        if let Some(front) = frontmost() {
            assert!(!front.bundle_id.is_empty());
            assert!(
                front.bundle_id.contains('.'),
                "not a bundle id: {}",
                front.bundle_id
            );
        }
    }
}
