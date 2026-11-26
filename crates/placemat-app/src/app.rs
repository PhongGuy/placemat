//! Startup: the main window's options and opening it.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use gpui_kit::component::TitleBar;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, TitlebarOptions, WindowDecorations, WindowOptions, px,
    size,
};

use image::{ImageFormat, RgbaImage};

use crate::main_view::MainView;

/// The window's app id and X11 class: the stem and `StartupWMClass` of the installed
/// desktop entry.
pub const APP_ID: &str = "placemat";

/// The 256 px icon for X11's `_NET_WM_ICON`, decoded from the committed
/// `assets/icon/generated/png/256x256.png` on first use and shared after; None if it fails
/// to decode, and the window then opens without one.
pub fn window_icon() -> Option<Arc<RgbaImage>> {
    static ICON: OnceLock<Option<Arc<RgbaImage>>> = OnceLock::new();
    let png = include_bytes!("../../../assets/icon/generated/png/256x256.png");
    ICON.get_or_init(|| {
        image::load_from_memory_with_format(png, ImageFormat::Png)
            .ok()
            .map(|img| Arc::new(img.to_rgba8()))
    })
    .clone()
}

/// The smallest window, in logical pixels.
const MIN_WINDOW: (f32, f32) = (480., 320.);

/// The main window's options, built on the kit title bar's and titled "Placemat". On Linux
/// the app draws its own title bar, so the compositor does not add a second one.
pub fn window_options() -> WindowOptions {
    WindowOptions {
        window_min_size: Some(size(px(MIN_WINDOW.0), px(MIN_WINDOW.1))),
        window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
        titlebar: Some(TitlebarOptions {
            title: Some("Placemat".into()),
            ..TitleBar::title_bar_options()
        }),
        app_id: Some(APP_ID.into()),
        icon: window_icon(),
        ..TitleBar::window_options()
    }
}

/// Opens the main window, opening the board at `path` into it when one is named. The caller
/// has run `gpui_kit::init` and `theme::install`.
pub fn start(cx: &mut App, path: Option<PathBuf>) -> gpui_kit::Result<AnyWindowHandle> {
    let (window, _) = gpui_kit::open_window(window_options(), cx, |window, cx| {
        // X11 takes its window class only from this call, never from the options' app id.
        window.set_app_id(APP_ID);
        cx.new(|cx| MainView::new(window, path, cx))
    })?;
    Ok(window)
}
