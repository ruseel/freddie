//! A borderless overlay panel of monospaced text, click-through and non-activating.
//!
//! [`overlay`] builds one on the main thread. [`OverlaySink::show`] / [`hide`] send over a
//! [`freddie_main_loop::WakingSender`]; [`Overlay::pump`] applies the change on `on_wake`.
//! Needs `freddie_main_loop` running and `NSApp` initialized. macOS only.

use std::sync::mpsc::Receiver;

use freddie_main_loop::{MainWaker, WakingSender};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSPanel, NSScreen, NSTextAlignment, NSTextField, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use tracing::debug;

const FONT_SIZE: f64 = 36.0;
const PADDING: f64 = 32.0;
const MARGIN: f64 = 20.0;
const BACKGROUND_ALPHA: f64 = 0.7;

struct Panel {
    panel: Retained<NSPanel>,
    label: Retained<NSTextField>,
}

enum OverlayMsg {
    Show(String),
    Hide,
}

/// Owns the panel. `!Send`. Dropping it closes the panel. A worker uses the [`OverlaySink`].
pub struct Overlay {
    panel: Panel,
    message_receiver: Receiver<OverlayMsg>,
}

/// `Send` and `Clone`. Safe to keep past its [`Overlay`]: a send after drop is a no-op.
#[derive(Clone)]
pub struct OverlaySink {
    message_sender: WakingSender<OverlayMsg>,
}

impl std::fmt::Debug for OverlaySink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlaySink").finish_non_exhaustive()
    }
}

/// Build a hidden overlay panel and return the handle that owns it beside the first sink.
/// The panel is built immediately, so a later show does not construct one.
///
/// # Panics
///
/// If called off the main thread.
#[must_use]
pub fn overlay(waker: &MainWaker) -> (Overlay, OverlaySink) {
    let mtm = MainThreadMarker::new().expect("overlay must be built on the main thread");
    let (message_sender, message_receiver) = waker.channel();
    debug!("overlay built");
    (
        Overlay {
            panel: build(mtm),
            message_receiver,
        },
        OverlaySink { message_sender },
    )
}

impl Overlay {
    /// Apply every queued show/hide to the panel. Call on the main thread, from `on_wake`.
    ///
    /// # Panics
    ///
    /// If called off the main thread, where the panel cannot be touched.
    pub fn pump(&self) {
        let mtm = MainThreadMarker::new().expect("Overlay::pump must run on the main thread");
        let Panel { panel, label } = &self.panel;
        for msg in self.message_receiver.try_iter() {
            match msg {
                OverlayMsg::Show(text) => {
                    // A keymap file ends with a newline, which would draw as a blank last row.
                    label.setStringValue(&NSString::from_str(text.trim_end()));
                    label.sizeToFit();
                    resize_to_label(panel, label);
                    place(panel, mtm);
                    panel.orderFrontRegardless();
                    debug!(text, "overlay shown");
                }
                OverlayMsg::Hide => {
                    panel.orderOut(None);
                    debug!("overlay hidden");
                }
            }
        }
    }
}

impl Drop for Overlay {
    /// `close` takes the panel off `AppKit`'s window list. Dropping the `Retained` alone would
    /// leave it on screen. `releasedWhenClosed` is false, so the release is this `Retained`'s.
    fn drop(&mut self) {
        self.panel.panel.close();
        debug!("overlay closed");
    }
}

impl OverlaySink {
    /// Show the overlay with `text`, from any thread.
    pub fn show(&self, text: String) {
        let _ = self.message_sender.send(OverlayMsg::Show(text));
    }

    /// Hide the overlay, from any thread. The panel stays built.
    pub fn hide(&self) {
        let _ = self.message_sender.send(OverlayMsg::Hide);
    }
}

fn build(mtm: MainThreadMarker) -> Panel {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0));
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    // SAFETY: the NSPanel designated initializer, on the main thread.
    let panel = {
        NSPanel::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: standard panel configuration, on the main thread.
    {
        // `NSScreenSaverWindowLevel` is 1000.
        panel.setLevel(1000);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setIgnoresMouseEvents(true);
        panel.setHidesOnDeactivate(false);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
    }

    // SAFETY: a layer-backed container drawing the rounded dark background, on the main thread.
    let container = {
        let view = NSView::initWithFrame(mtm.alloc(), frame);
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setBackgroundColor(Some(&CGColor::new_generic_gray(0.0, BACKGROUND_ALPHA)));
            layer.setCornerRadius(10.0);
        }
        view
    };

    // SAFETY: a non-editable, non-bezeled, multi-line monospaced label, on the main thread.
    let label = {
        let label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        label.setAlignment(NSTextAlignment::Left);
        label.setTextColor(Some(&NSColor::whiteColor()));
        label.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(
            FONT_SIZE, 0.0,
        )));
        label.setUsesSingleLineMode(false);
        label.setMaximumNumberOfLines(0);
        label.setDrawsBackground(false);
        label.setBezeled(false);
        label.setEditable(false);
        label.setSelectable(false);
        label
    };
    // SAFETY: installing the label in the container and the container in the panel, on main.
    {
        container.addSubview(&label);
        panel.setContentView(Some(&container));
    }
    // SAFETY: on the main thread, before anything else holds the panel.
    #[expect(unsafe_code)]
    unsafe {
        // `NSWindow` defaults to releasing itself when closed, which would double-release
        // the `Retained` in `Overlay::drop`.
        panel.setReleasedWhenClosed(false);
    }
    Panel { panel, label }
}

fn resize_to_label(panel: &NSPanel, label: &NSTextField) {
    // SAFETY: reading the fitted label and resizing the panel and its views, on the main thread.
    {
        let text = label.frame().size;
        let size = NSSize::new(
            PADDING.mul_add(2.0, text.width),
            PADDING.mul_add(2.0, text.height),
        );
        panel.setContentSize(size);
        if let Some(container) = panel.contentView() {
            container.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), size));
        }
        label.setFrameOrigin(NSPoint::new(PADDING, PADDING));
    }
}

fn place(panel: &NSPanel, mtm: MainThreadMarker) {
    let Some(screen) = NSScreen::mainScreen(mtm) else {
        return;
    };
    // SAFETY: reading the screen's visible frame and moving the panel, on the main thread.
    {
        let vis = screen.visibleFrame();
        let size = panel.frame().size;
        let x = vis.origin.x + vis.size.width - size.width - MARGIN;
        let y = vis.origin.y + (vis.size.height - size.height) / 2.0;
        panel.setFrameOrigin(NSPoint::new(x, y));
    }
}
