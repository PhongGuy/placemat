//! The in-app updater: one notice offering a newer release, staged on Install and swapped
//! in at quit.

mod model;
mod quit;

use std::time::Duration;

use gpui_kit::{App, AppContext as _, BorrowAppContext as _, Global, Task};
use placemat_core::update::{
    self, Available, InstallKind, Os, Reason, Staged, UpdateConfig, install_kind, staging_dir,
    sweep_staged,
};

pub use model::{Armed, Notice, Offer, UpdateModel};
pub use quit::{Launcher, QuitInstall, SystemLauncher, install_at_quit, relaunch_command};

/// How long the checker waits between feed checks.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The running updater: the notice model, the config its checks use and the tick loop. It
/// exists only when update checks are enabled.
pub struct Updater {
    model: UpdateModel<Available, Staged>,
    cfg: UpdateConfig,
    _ticks: Task<()>,
}

impl Global for Updater {}

impl Updater {
    /// The notice model the main view renders.
    pub fn model(&self) -> &UpdateModel<Available, Staged> {
        &self.model
    }
}

/// How this process is installed, judged from its canonical executable and `$APPIMAGE`;
/// an unknown system or executable path is `Unrecognised`.
pub fn local_kind() -> InstallKind {
    let exe = std::env::current_exe().and_then(std::fs::canonicalize);
    let (Some(os), Ok(exe)) = (Os::current(), exe) else {
        return InstallKind::NotOwned(Reason::Unrecognised);
    };
    let appimage = std::env::var_os("APPIMAGE").and_then(|p| std::fs::canonicalize(p).ok());
    install_kind(os, update::RELEASE_BUILD, &exe, appimage.as_deref())
}

/// Removes staged downloads and `.part` files left beside the install `kind` by an earlier
/// session; nothing for an install the updater does not own.
pub fn sweep_at_launch(kind: &InstallKind) {
    if let Ok(dir) = staging_dir(kind) {
        sweep_staged(&dir);
    }
}

/// Sweeps leftovers, then, when checks are enabled, sets the `Updater` global and starts
/// the checker: once now and every 24 hours, failures logged and retried at the next tick.
pub fn init(cx: &mut App) {
    let kind = local_kind();
    sweep_at_launch(&kind);
    // Disabled (opt-out, a source build without the test feed, no keys, no target) shows nothing.
    let Ok(cfg) = update::build_config() else {
        return;
    };
    // The global exists before the loop first runs, so the launch check is not skipped.
    cx.set_global(Updater {
        model: UpdateModel::new(kind),
        cfg,
        _ticks: Task::ready(()),
    });
    // Annotated: the loop's type is `!`, and edition 2024 does not fall back to `()`.
    let ticks: Task<()> = cx.spawn(async move |cx| {
        loop {
            let cfg = cx.update(|cx| {
                cx.try_global::<Updater>()
                    .filter(|u| u.model.should_check())
                    .map(|u| u.cfg.clone())
            });
            if let Some(cfg) = cfg {
                let found = cx
                    .background_spawn(async move { update::check(&cfg) })
                    .await;
                if let Err(e) = &found {
                    eprintln!("placemat: update check failed: {e}");
                }
                let offer =
                    found.map(|f| f.map(|a| Offer::new(a.version().to_owned(), a.format(), a)));
                cx.update(|cx| cx.update_global::<Updater, _>(|u, _| u.model.checked(offer)));
            }
            cx.background_executor().timer(CHECK_EVERY).await;
        }
    });
    cx.global_mut::<Updater>()._ticks = ticks;
}

/// Installs a staged update synchronously, relaunching when asked; nothing without the
/// global. Runs from the quit hook's body.
pub fn quit(cx: &mut App) {
    // `global_mut` panics without the global, so a disabled updater passes no model.
    let model = if cx.has_global::<Updater>() {
        Some(&mut cx.global_mut::<Updater>().model)
    } else {
        None
    };
    match install_at_quit(model, update::install, &SystemLauncher) {
        QuitInstall::Nothing | QuitInstall::Installed => {}
        QuitInstall::InstallFailed(e) => eprintln!("placemat: update install failed: {e}"),
        QuitInstall::RelaunchFailed(e) => eprintln!("placemat: relaunch after update failed: {e}"),
    }
}

/// Install: downloads the offered release in the background and records the outcome;
/// nothing unless an installable offer is shown.
pub fn start_install(cx: &mut App) {
    if !cx.has_global::<Updater>() {
        return;
    }
    let (payload, dir) = cx.update_global::<Updater, _>(|u, _| {
        let payload = u.model.start_download().cloned();
        (payload, staging_dir(u.model.kind()))
    });
    let Some(payload) = payload else {
        return;
    };
    let dir = match dir {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("placemat: update download: {e}");
            cx.update_global::<Updater, _>(|u, _| u.model.downloaded(None));
            return;
        }
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move { update::download(&payload, &dir, |_, _| {}) })
            .await;
        if let Err(e) = &result {
            eprintln!("placemat: update download failed: {e}");
        }
        cx.update(|cx| cx.update_global::<Updater, _>(|u, _| u.model.downloaded(result.ok())));
    })
    .detach();
}

/// Later: hides the notice for the rest of the session.
pub fn dismiss(cx: &mut App) {
    if cx.has_global::<Updater>() {
        cx.update_global::<Updater, _>(|u, _| u.model.later());
    }
}

/// Restart now: asks quit to relaunch after the install, then quits.
pub fn restart_now(cx: &mut App) {
    if !cx.has_global::<Updater>() {
        return;
    }
    if cx.update_global::<Updater, _>(|u, _| u.model.restart_now()) {
        cx.quit();
    }
}

/// Opens the release page in the browser.
pub fn open_download_page(cx: &mut App) {
    cx.open_url(update::DOWNLOAD_PAGE);
}
