//! Placemat under gpui-pilot's headless host: the launch glue an agent drives.

use std::sync::Arc;

use gpui_kit::{App, AssetSource};
use gpui_pilot_host::{Launch, PilotApp};

/// Placemat's own startup, run inside the host; the launch's size and scale come from the pilot.
pub struct Placemat;

impl PilotApp for Placemat {
    fn assets(&self) -> Option<Arc<dyn AssetSource>> {
        Some(Arc::new(placemat_app::ui::AppAssets))
    }

    fn init(&self, cx: &mut App) {
        gpui_kit::init(cx);
        placemat_app::theme::install(cx);
    }

    fn open(&self, _launch: &Launch, cx: &mut App) -> anyhow::Result<()> {
        placemat_app::app::start(cx)?;
        Ok(())
    }
}
