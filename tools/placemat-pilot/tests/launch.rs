//! The first scenario: the app launches under the pilot and shows its one empty window.

mod common;

use common::{Pilot, launch_args, node};
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn the_app_opens_one_empty_window_with_the_placemat_placeholder() {
    let pilot = Pilot::start().await;
    pilot.ok("launch", launch_args("headless")).await;
    let tree = pilot.ok("tree", json!({})).await;
    assert_eq!(
        tree["settled"],
        json!(true),
        "the app never settled: {tree}"
    );
    node(&tree, "main");
    assert_eq!(
        node(&tree, "placeholder")["name"],
        json!("Placemat"),
        "{tree}"
    );
    pilot.assert_bus_untouched();
}
