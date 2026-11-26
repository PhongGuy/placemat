//! A board opened from the command line: its canvas, leaves and input.

mod common;

use common::{Pilot, find_node, fixture, launch_with, node};
use serde_json::json;

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
