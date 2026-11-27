//! The in-app updater: one notice offering a newer release, staged on Install and swapped
//! in at quit.

mod model;

pub use model::{Armed, Notice, Offer, UpdateModel};
