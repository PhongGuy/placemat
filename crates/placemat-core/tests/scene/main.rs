//! The scene integration suite: one binary, one module per concern. A new file must be declared
//! below, or it never runs.
// Helpers may panic on a broken fixture; clippy's in-tests exemption covers #[test] fns only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
#[path = "../board/util.rs"]
mod board_util;
#[allow(dead_code)]
#[path = "../board/boardgen.rs"]
mod boardgen;
mod cull;
mod extreme;
mod golden;
mod util;
