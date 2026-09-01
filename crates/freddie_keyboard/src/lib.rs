//! Keyboard interception and emission on macOS via `core-graphics`.
//!
//! [`intercept`] grabs the keyboard and hands back an [`Interceptor`] and an [`Emitter`].
//! The interceptor's callback returns `Some(same)` to pass, `Some(other)` to remap, or
//! `None` to drop. The emitter synthesizes keys not tied to an intercepted event.

use std::fmt;

pub use freddie_keys::{Key, KeyEvent, PressType};

#[cfg(target_os = "macos")]
pub use freddie_hid_device::{DeviceInfo, ResolveFailure, SourceId};

mod sys;
pub use sys::{
    Emitter, Interceptor, MouseInterceptor, Tag, intercept, intercept_mouse, intercept_with_source,
};

/// The keyboard could not be intercepted. Usually Accessibility is not granted.
#[derive(Debug)]
pub struct CaptureError;

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("could not intercept the keyboard (is Accessibility granted?)")
    }
}

impl std::error::Error for CaptureError {}

/// A key could not be emitted.
#[derive(Debug)]
pub enum EmitError {
    /// The key has no code on this OS, so it cannot be emitted.
    Unmappable(Key),
    /// The OS refused to build or post the event.
    Post,
}

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unmappable(key) => write!(f, "{key:?} has no key code on this OS"),
            Self::Post => f.write_str("could not build or post the key event"),
        }
    }
}

impl std::error::Error for EmitError {}
