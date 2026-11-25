//! The app's asset source: gpui-kit's bundled icons.

use std::borrow::Cow;

use gpui_kit::assets::Assets;
use gpui_kit::{AssetSource, SharedString};

/// The assets the app draws with. A path the kit's bundle lacks is absent rather than an
/// error, so GPUI falls back instead of failing the frame.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        Ok(Assets.load(path).ok().flatten())
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        Assets.list(path)
    }
}
