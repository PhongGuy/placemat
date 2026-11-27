//! The updater core: config, signature, manifest rules, check, download and install.
// Test helpers may panic on a broken fixture too; clippy's in-tests exemption covers only
// #[test] functions, not the helpers they call.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod config;
mod evaluate;
mod signature;
mod support;
