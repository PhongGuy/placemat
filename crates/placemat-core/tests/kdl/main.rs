//! The KDL reader's integration suite: one binary, one module per concern.
// Test helpers may panic on a broken fixture too; clippy's in-tests exemption covers only
// #[test] functions, not the helpers they call.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod reader;
