//! Opening boards from the window: the empty state, the file dialog and its shortcut, and the
//! notes an open leaves behind.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{Pilot, find_node, fixture, launch_args, launch_with, leaves, node, segment};
use placemat_core::board::{LoadError, OpenError, read_file};
use serde_json::{Value, json};

const RECT: [&str; 2] = ["shape-3kTq8wZp", "shape-7hWc4nRd"];
const ELLIPSE: [&str; 1] = ["shape-b2Dy6sKu"];

fn shapes(tree: &Value) -> BTreeSet<String> {
    leaves(tree)
        .into_iter()
        .map(|n| segment(n).to_owned())
        .collect()
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|s| (*s).to_owned()).collect()
}

fn name<'a>(tree: &'a Value, id: &str) -> &'a str {
    node(tree, id)["name"]
        .as_str()
        .unwrap_or_else(|| panic!("{id} has no name: {tree}"))
}

/// Answers the open dialog the app is showing with `path`; fails if none was showing.
async fn pick(pilot: &Pilot, path: &Path) {
    let answer = pilot.ok("pick_file", json!({"paths": [path]})).await;
    assert_eq!(
        answer["answered"],
        json!("open"),
        "no dialog was showing: {answer}"
    );
}

/// The tree once element `id` is visible.
async fn tree_with(pilot: &Pilot, id: &str) -> Value {
    pilot.ok("wait", json!({"target": {"id": [id]}})).await;
    pilot.ok("tree", json!({})).await
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_board_the_empty_state_offers_open_board() {
    let pilot = Pilot::start().await;
    pilot.ok("launch", launch_args("headless")).await;
    let tree = tree_with(&pilot, "open-board").await;
    assert_eq!(name(&tree, "placeholder"), "Placemat");
    assert_eq!(name(&tree, "open-board"), "Open board");
    assert!(find_node(&tree, "board").is_none(), "{tree}");
    pilot.assert_bus_untouched();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_open_button_opens_the_picked_board() {
    let pilot = Pilot::start().await;
    pilot.ok("launch", launch_args("headless")).await;
    pilot
        .ok("click", json!({"target": {"id": ["open-board"]}}))
        .await;
    pick(&pilot, &fixture("canonical/rect.board")).await;
    let tree = tree_with(&pilot, "board").await;
    assert!(find_node(&tree, "placeholder").is_none(), "{tree}");
    assert_eq!(shapes(&tree), set(&RECT));
}

#[tokio::test(flavor = "multi_thread")]
async fn ctrl_o_opens_the_dialog_and_a_cancel_changes_nothing() {
    let pilot = Pilot::start().await;
    pilot.ok("launch", launch_args("headless")).await;
    pilot
        .ok("wait", json!({"target": {"id": ["open-board"]}}))
        .await;
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    let cancel = pilot.ok("pick_file", json!({"cancel": true})).await;
    assert_eq!(
        cancel["answered"],
        json!("cancelled"),
        "ctrl-o showed no dialog: {cancel}"
    );
    let tree = pilot.ok("tree", json!({})).await;
    node(&tree, "open-board");
    assert!(find_node(&tree, "board").is_none(), "{tree}");
    assert!(
        find_node(&tree, "board-error").is_none(),
        "a cancel is not an error: {tree}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_open_replaces_the_first_from_either_focus() {
    let pilot = Pilot::start().await;
    let rect = fixture("canonical/rect.board");
    pilot.ok("launch", launch_with("headless", &[&rect])).await;
    pilot.ok("wait", json!({"target": {"id": ["board"]}})).await;
    // First with the window's own focus, then once a click has focused the board.
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    pick(&pilot, &fixture("canonical/ellipse.board")).await;
    let tree = tree_with(&pilot, "shape-b2Dy6sKu").await;
    assert_eq!(shapes(&tree), set(&ELLIPSE));
    pilot
        .ok("click", json!({"target": {"id": ["board"]}}))
        .await;
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    pick(&pilot, &rect).await;
    let tree = tree_with(&pilot, "shape-3kTq8wZp").await;
    assert_eq!(shapes(&tree), set(&RECT));
    // The focused board is gone with the replace; the key must still reach the window.
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    let cancel = pilot.ok("pick_file", json!({"cancel": true})).await;
    assert_eq!(
        cancel["answered"],
        json!("cancelled"),
        "ctrl-o after a replace showed no dialog: {cancel}"
    );
}

/// Waits until element `id` is no longer visible.
async fn gone(pilot: &Pilot, id: &str) {
    pilot
        .ok("wait", json!({"target": {"id": [id]}, "gone": true}))
        .await;
}

/// A board whose rect node never closes, and where the loader places the error.
fn broken(dir: &Path) -> (PathBuf, String) {
    let path = dir.join("broken.board");
    std::fs::write(
        &path,
        "placemat-board version=0\n\nrect \"a\" {\n    at 0 0\n",
    )
    .unwrap();
    let at = match read_file(&path) {
        Err(OpenError::Load(LoadError::Syntax { line, col, .. })) => {
            format!("line {line}, column {col}")
        }
        other => panic!("the fixture must be a syntax error: {other:?}"),
    };
    (path, at)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_broken_board_shows_why_with_its_line_and_column() {
    let dir = tempfile::TempDir::new().unwrap();
    let (path, at) = broken(dir.path());
    let pilot = Pilot::start().await;
    pilot.ok("launch", launch_with("headless", &[&path])).await;
    let tree = tree_with(&pilot, "board-error").await;
    let why = name(&tree, "board-error");
    assert!(why.contains("broken.board") && why.contains(&at), "{why}");
    node(&tree, "open-board");
    assert!(find_node(&tree, "board").is_none(), "{tree}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_second_open_keeps_the_board_until_one_succeeds() {
    let dir = tempfile::TempDir::new().unwrap();
    let pilot = Pilot::start().await;
    let rect = fixture("canonical/rect.board");
    pilot.ok("launch", launch_with("headless", &[&rect])).await;
    pilot.ok("wait", json!({"target": {"id": ["board"]}})).await;
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    pick(&pilot, dir.path()).await;
    let tree = tree_with(&pilot, "board-error").await;
    assert!(
        name(&tree, "board-error").contains("not a regular file"),
        "{tree}"
    );
    assert_eq!(shapes(&tree), set(&RECT), "the shown board stays");
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    pick(&pilot, &fixture("canonical/ellipse.board")).await;
    gone(&pilot, "board-error").await;
    let tree = pilot.ok("tree", json!({})).await;
    assert_eq!(shapes(&tree), set(&ELLIPSE));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repaired_board_opens_with_a_dismissible_warnings_banner() {
    let pilot = Pilot::start().await;
    let path = fixture("repair/duplicate.board");
    pilot.ok("launch", launch_with("headless", &[&path])).await;
    let tree = tree_with(&pilot, "board-warnings").await;
    let listed = name(&tree, "board-warnings");
    assert!(
        listed.contains("a1Xq") && listed.contains("renamed"),
        "{listed}"
    );
    node(&tree, "board");
    pilot
        .ok(
            "click",
            json!({"target": {"id": ["board-warnings-dismiss"]}}),
        )
        .await;
    gone(&pilot, "board-warnings").await;
    node(&pilot.ok("tree", json!({})).await, "board");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_newer_board_opens_read_only_and_a_clean_open_clears_the_notes() {
    let pilot = Pilot::start().await;
    let path = fixture("readonly/newer.board");
    pilot.ok("launch", launch_with("headless", &[&path])).await;
    let tree = tree_with(&pilot, "board-readonly").await;
    assert!(
        name(&tree, "board-readonly").contains("made by a newer Placemat"),
        "{tree}"
    );
    node(&tree, "board");
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    pick(&pilot, &fixture("canonical/rect.board")).await;
    gone(&pilot, "board-readonly").await;
    let tree = pilot.ok("tree", json!({})).await;
    assert!(find_node(&tree, "board-warnings").is_none(), "{tree}");
    assert_eq!(shapes(&tree), set(&RECT));
}

/// Opens `board`, waits for its `note` banner, then fails a second open and checks that the
/// note and the board stay beside the error.
async fn a_failed_open_keeps(board: &str, note: &str) {
    let dir = tempfile::TempDir::new().unwrap();
    let (path, _) = broken(dir.path());
    let pilot = Pilot::start().await;
    pilot
        .ok("launch", launch_with("headless", &[&fixture(board)]))
        .await;
    tree_with(&pilot, note).await;
    pilot.ok("key", json!({"keys": "ctrl-o"})).await;
    pick(&pilot, &path).await;
    let tree = tree_with(&pilot, "board-error").await;
    node(&tree, note);
    node(&tree, "board");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_open_keeps_the_warnings_banner() {
    a_failed_open_keeps("repair/duplicate.board", "board-warnings").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_open_keeps_the_read_only_note() {
    a_failed_open_keeps("readonly/newer.board", "board-readonly").await;
}
