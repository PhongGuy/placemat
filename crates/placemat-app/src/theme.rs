//! Installs the bundled fonts and the Placemat light and dark themes into gpui-kit's global
//! `Theme`, and keeps the kit's mode on the one the OS window appearance asks for.

use std::borrow::Cow;

use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{App, Window, WindowAppearance};

/// The shipped theme set: one dark and one light theme.
pub const THEME_JSON: &str = include_str!("../assets/theme/placemat.json");
/// The name of the shipped dark theme.
pub const DARK: &str = "Placemat Dark";
/// The name of the shipped light theme.
pub const LIGHT: &str = "Placemat Light";
/// The bundled TrueType fonts: Inter Regular, Medium, SemiBold and Italic.
pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Italic.ttf"),
];

/// Registers the bundled fonts and themes and makes the themes the kit's light and dark
/// themes. Fonts or a theme that fail to load leave the system fonts or the kit default and
/// log to stderr; calling it again changes nothing.
pub fn install(cx: &mut App) {
    // Fonts go first so the theme reload below already resolves "Inter" on the first frame.
    let fonts = FONTS.iter().map(|f| Cow::Borrowed(*f)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("placemat: bundled fonts failed to register ({err}); using system fonts");
    }
    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEME_JSON) {
        eprintln!("placemat: the bundled theme failed to load: {err}");
        return;
    }
    let themes = ThemeRegistry::global(cx).themes();
    let (dark, light) = (themes.get(DARK).cloned(), themes.get(LIGHT).cloned());
    Theme::update(cx, |t| {
        if let Some(dark) = dark {
            t.dark_theme = dark;
        }
        if let Some(light) = light {
            t.light_theme = light;
        }
    });
    // `change` reloads the current mode's config even when the mode is unchanged.
    Theme::change(Theme::global(cx).mode, None, cx);
}

/// Puts the kit in the mode `os` asks for. A mode change refreshes every window, so it
/// happens only on a difference.
pub fn follow(os: WindowAppearance, window: &mut Window, cx: &mut App) {
    let mode = ThemeMode::from(os);
    if Theme::global(cx).mode != mode {
        Theme::change(mode, Some(window), cx);
    }
}
