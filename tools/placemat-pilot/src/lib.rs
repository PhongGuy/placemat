//! Placemat under gpui-pilot's headless host: the launch glue an agent drives.

use std::sync::Arc;

use anyhow::bail;
use gpui_kit::{App, AssetSource};
use gpui_pilot_host::{Launch as PilotLaunch, PilotApp};
use placemat_app::launch::{self, Launch};

/// Placemat's own startup, run inside the host; the launch's size and scale come from the pilot.
pub struct Placemat;

impl PilotApp for Placemat {
    fn assets(&self) -> Option<Arc<dyn AssetSource>> {
        Some(Arc::new(placemat_app::ui::AppAssets))
    }

    fn init(&self, cx: &mut App) {
        gpui_kit::init(cx);
        placemat_app::theme::install(cx);
        placemat_app::viewer::init(cx);
        placemat_app::app::init(cx);
    }

    /// Opens the window as the app's own command line would, so `launch.args` name the board.
    fn open(&self, launch: &PilotLaunch, cx: &mut App) -> anyhow::Result<()> {
        let path = match launch::parse(launch.args.iter().map(Into::into))? {
            Launch::Open { path } => path,
            other => bail!("the pilot host only opens the window, not {other:?}"),
        };
        placemat_app::app::start(cx, path)?;
        Ok(())
    }
}
