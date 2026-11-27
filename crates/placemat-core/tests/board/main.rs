//! The board integration suite.
//!
//! Each file beside this one is a module of this single binary, so the
//! suite links once. A new test file must be declared below, or it never runs.
// Test helpers may panic on a broken fixture too; clippy's in-tests exemption covers only
// #[test] functions, not the helpers they call.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod boardgen;
mod edit;
mod fixtures;
mod load;
mod merge_lab;
mod model;
mod open;
mod order;
mod repair;
mod shown;
mod unknown;
mod util;
mod write;
