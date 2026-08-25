//! A macOS menu-bar status item with a single Quit entry.
//!
//! Call [`show`] on the main thread after `NSApp` is initialized
//! (`freddie_main_loop::init_menu_bar_app`). The returned [`MenuBar`] is `!Send`; dropping
//! it removes the icon.

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// How the status-item PNG is drawn.
///
/// [`IconKind::Template`] is a monochrome glyph: macOS ignores RGB and paints the alpha
/// mask in the menu bar's color. [`IconKind::Color`] keeps the PNG's colors.
#[derive(Clone, Copy)]
pub enum IconKind<'a> {
    Template(&'a [u8]),
    Color(&'a [u8]),
}

/// A live status item. Dropping it takes the icon down and clears the menu handler.
pub struct MenuBar {
    tray: TrayIcon,
}

impl Drop for MenuBar {
    /// Clears the process-global menu handler this `MenuBar` installed.
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
    }
}

impl MenuBar {
    /// Set the text shown beside the glyph, or clear it with `None`. Main thread only.
    pub fn set_title(&self, title: Option<&str>) {
        self.tray.set_title(title);
    }
}

/// Show a menu-bar status item with a single Quit entry.
///
/// The menu handler is process-global, so a second `MenuBar` replaces the first one's.
/// `on_quit` runs on the main thread when the user chooses Quit.
///
/// # Errors
///
/// If the icon, the menu, or the status item cannot be created.
pub fn show(
    tooltip: &str,
    icon: IconKind<'_>,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> Result<MenuBar, Box<dyn std::error::Error + Send + Sync>> {
    let quit = MenuItem::new("Quit", true, None);
    let quit_id = quit.id().clone();

    let menu = Menu::new();
    menu.append(&quit)?;

    let (png, as_template) = match icon {
        IconKind::Template(png) => (png, true),
        IconKind::Color(png) => (png, false),
    };

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(prepare_icon(png)?)
        .with_icon_as_template(as_template)
        .with_tooltip(tooltip)
        .build()?;

    // muda delivers menu events through one global handler, on the main thread.
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == quit_id {
            on_quit();
        }
    }));

    Ok(MenuBar { tray })
}

/// Trim `png` to its opaque bounds and size it for the menu bar.
fn prepare_icon(png: &[u8]) -> Result<Icon, Box<dyn std::error::Error + Send + Sync>> {
    let img = image::load_from_memory(png)?.into_rgba8();
    let glyph = crop_to_alpha(&img);

    // tray-icon renders at 18pt; ~2x pixel height stays crisp on a Retina bar.
    let glyph_h: u32 = 30;
    let glyph_w = (glyph.width() * glyph_h)
        .div_ceil(glyph.height().max(1))
        .max(1);
    let scaled = image::imageops::resize(
        &glyph,
        glyph_w,
        glyph_h,
        image::imageops::FilterType::Lanczos3,
    );

    let pad: u32 = 4;
    let mut canvas = image::RgbaImage::new(glyph_w + 2 * pad, glyph_h + 2 * pad);
    image::imageops::overlay(&mut canvas, &scaled, i64::from(pad), i64::from(pad));

    let (w, h) = (canvas.width(), canvas.height());
    Ok(Icon::from_rgba(canvas.into_raw(), w, h)?)
}

/// Crop to the bounding box of non-transparent pixels. Fully transparent: return a clone.
fn crop_to_alpha(img: &image::RgbaImage) -> image::RgbaImage {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0_u32, 0_u32);
    for (x, y, px) in img.enumerate_pixels() {
        if px.0[3] > 16 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x > max_x {
        return img.clone();
    }
    image::imageops::crop_imm(img, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1).to_image()
}
