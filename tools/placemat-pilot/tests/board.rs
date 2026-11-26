//! A board opened from the command line: its canvas, leaves and input.

mod common;

use common::{Pilot, find_node, fixture, launch_with, leaves, meets, node, rect, segment};
use serde_json::{Value, json};

/// The path segments above a leaf, as gpui-pilot v0.9.1 reports them; pinned from a real run.
const LEAF_PREFIX: &[&str] = &[];

#[tokio::test(flavor = "multi_thread")]
async fn a_board_path_opens_the_board_in_place_of_the_placeholder() {
    let pilot = Pilot::start().await;
    let path = fixture("perf/grid-1000.board");
    pilot.ok("launch", launch_with("headless", &[&path])).await;
    let tree = pilot
        .ok("tree", json!({"wait_for": {"id": ["board"]}}))
        .await;
    assert_eq!(
        tree["settled"],
        json!(true),
        "the app never settled: {tree}"
    );
    node(&tree, "board");
    assert!(find_node(&tree, "placeholder").is_none(), "{tree}");
    pilot.assert_bus_untouched();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_that_does_not_open_leaves_the_placeholder() {
    let pilot = Pilot::start().await;
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("missing.board");
    pilot
        .ok("launch", launch_with("headless", &[&missing]))
        .await;
    let tree = pilot.ok("tree", json!({})).await;
    node(&tree, "placeholder");
    assert!(find_node(&tree, "board").is_none(), "{tree}");
    let mut again = launch_with("headless", &[dir.path()]);
    again["replace"] = json!(true);
    pilot.ok("launch", again).await;
    let tree = pilot.ok("tree", json!({})).await;
    node(&tree, "placeholder");
    assert!(
        find_node(&tree, "board").is_none(),
        "a directory is not a board: {tree}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_canvas_lists_one_leaf_per_visible_shape() {
    let pilot = Pilot::start().await;
    let path = fixture("perf/grid-1000.board");
    pilot.ok("launch", launch_with("headless", &[&path])).await;
    let tree = pilot
        .ok("tree", json!({"wait_for": {"id": ["board"]}}))
        .await;
    let board = rect(node(&tree, "board"));
    let found = leaves(&tree);
    let loaded = placemat_core::board::read_file(&path).unwrap();
    let order: Vec<String> = loaded
        .board
        .paint_order()
        .map(|e| format!("shape-{}", e.id.as_str()))
        .collect();
    let mut ids: Vec<&str> = found.iter().map(|n| segment(n)).collect();
    let mut order: Vec<&str> = order.iter().map(String::as_str).collect();
    ids.sort_unstable();
    order.sort_unstable();
    assert_eq!(
        ids, order,
        "at fit every shape is a leaf (order is unit-tested in frame.rs)"
    );
    for n in &found {
        let mut want: Vec<Value> = LEAF_PREFIX.iter().map(|s| json!(s)).collect();
        want.push(json!(segment(n)));
        assert_eq!(n["path"], Value::Array(want), "{n}");
        assert!(meets(rect(n), board), "{n} lies off the canvas {board:?}");
    }
    let by = |id: &str| *found.iter().find(|n| segment(n) == id).unwrap();
    assert_eq!(by("shape-g00000")["role"], json!("Group"));
    assert_eq!(by("shape-g00001")["role"], json!("Group"));
    assert_eq!(by("shape-g00002")["role"], json!("Image"));
    assert_eq!(by("shape-g00003")["role"], json!("Image"));
    assert_eq!(by("shape-g00001")["name"], json!("Ellipse g00001"));
    assert_eq!(by("shape-g00999")["name"], json!("Label 999"));
}
