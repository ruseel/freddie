//! Backend selection. macOS on `core-graphics` is the only backend.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{
    Emitter, Interceptor, MouseInterceptor, intercept, intercept_mouse, intercept_with_source,
};

#[cfg(not(target_os = "macos"))]
compile_error!("freddie_keyboard only has a macOS backend so far");
