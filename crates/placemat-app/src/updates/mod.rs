//! The in-app updater: one notice offering a newer release, staged on Install and swapped
//! in at quit.

mod model;
mod quit;

pub use model::{Armed, Notice, Offer, UpdateModel};
pub use quit::{Launcher, QuitInstall, SystemLauncher, install_at_quit, relaunch_command};
