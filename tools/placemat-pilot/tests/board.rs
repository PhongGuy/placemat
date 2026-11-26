//! A board opened from the command line: its canvas, leaves and input.

mod common;

use common::{Pilot, find_node, fixture, launch_with, leaves, meets, node, rect, segment};
use serde_json::{Value, json};

/// The path segments above a leaf, as gpui-pilot v0.9.1 reports them; pinned from a real run.
const LEAF_PREFIX: &[&str] = &[];

#[allow(dead_code)]
#[path = "../../../crates/placemat-core/tests/board/util.rs"]
mod board_util;

type Bx = (f64, f64, f64, f64);

fn boxes(tree: &Value) -> Vec<(String, Bx)> {
    leaves(tree)
        .into_iter()
        .map(|n| (segment(n).to_owned(), rect(n)))
        .collect()
}

fn at(list: &[(String, Bx)], id: &str) -> Option<Bx> {
    list.iter().find(|(i, _)| i == id).map(|(_, b)| *b)
}

fn centre(b: Bx) -> (f64, f64) {
    (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0)
}

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

#[tokio::test(flavor = "multi_thread")]
async fn a_drag_pans_the_canvas_and_the_fit_key_brings_it_back() {
    let pilot = Pilot::start().await;
    pilot
        .ok(
            "launch",
            launch_with("headless", &[&fixture("perf/grid-1000.board")]),
        )
        .await;
    let tree = pilot.ok("tree", json!({})).await;
    let board = rect(node(&tree, "board"));
    let before = boxes(&tree);
    let (cx, cy) = centre(board);
    pilot
        .ok(
            "drag",
            json!({"from": {"x": cx, "y": cy}, "to": {"x": cx, "y": cy + 300.0}}),
        )
        .await;
    let after = boxes(&pilot.ok("tree", json!({})).await);
    let bottom = board.1 + board.3;
    for (id, b) in &before {
        let top = b.1 + 300.0;
        match at(&after, id) {
            Some(a) if top + b.3 <= bottom - 1.0 => assert!(
                (a.0 - b.0).abs() < 0.5 && (a.1 - top).abs() < 0.5,
                "{id} moved {b:?} -> {a:?}, not by the drag"
            ),
            Some(_) => assert!(top < bottom + 1.0, "{id} at y {top} is past the canvas"),
            None => assert!(
                top >= bottom - 1.0,
                "{id} at y {top} left a canvas ending at {bottom}"
            ),
        }
    }
    assert!(
        after.len() < before.len(),
        "the drag pushed the bottom rows off"
    );
    assert!(
        after.iter().all(|(i, _)| at(&before, i).is_some()),
        "no shape appeared from nowhere"
    );
    pilot.ok("key", json!({"keys": "ctrl-0"})).await;
    let fitted = boxes(&pilot.ok("tree", json!({})).await);
    assert_eq!(fitted.len(), before.len());
    for (i, b) in &before {
        let a = at(&fitted, i).unwrap_or_else(|| panic!("{i} missing after fit"));
        assert!(
            (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5,
            "{i}: {a:?} vs {b:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wheel_zooms_about_the_cursor_and_shift_wheel_pans() {
    let pilot = Pilot::start().await;
    pilot
        .ok(
            "launch",
            launch_with("headless", &[&fixture("perf/grid-1000.board")]),
        )
        .await;
    let tree = pilot.ok("tree", json!({})).await;
    let (cx, cy) = centre(rect(node(&tree, "board")));
    let before = boxes(&tree);
    let dist = |b: &Bx| (centre(*b).0 - cx).powi(2) + (centre(*b).1 - cy).powi(2);
    let (id, b) = before
        .iter()
        .min_by(|x, y| dist(&x.1).total_cmp(&dist(&y.1)))
        .unwrap()
        .clone();
    pilot
        .ok(
            "scroll",
            json!({"target": {"x": cx, "y": cy}, "notches": -1}),
        )
        .await;
    let a = at(&boxes(&pilot.ok("tree", json!({})).await), &id).unwrap();
    assert!((a.2 / b.2 - 1.1).abs() < 0.01, "{id}: {b:?} -> {a:?}");
    assert!(
        (a.0 - (cx + (b.0 - cx) * 1.1)).abs() < 0.5,
        "x about the cursor: {a:?}"
    );
    assert!(
        (a.1 - (cy + (b.1 - cy) * 1.1)).abs() < 0.5,
        "y about the cursor: {a:?}"
    );
    pilot
        .ok(
            "scroll",
            json!({"target": {"x": cx, "y": cy}, "notches": 1, "modifiers": ["shift"]}),
        )
        .await;
    let c = at(&boxes(&pilot.ok("tree", json!({})).await), &id).unwrap();
    assert!(
        ((c.0 - a.0).abs() - 60.0).abs() < 0.5,
        "one notch pans 60 px: {a:?} -> {c:?}"
    );
    assert!(
        (c.1 - a.1).abs() < 0.5 && (c.2 - a.2).abs() < 0.01,
        "{a:?} -> {c:?}"
    );
}

/// Opens `board` at scale 2, screenshots it at fit and at full zoom, and returns the tree
/// read at fit.
async fn paint_at_fit_and_full_zoom(board: &placemat_core::board::Board) -> Value {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("big.board");
    let text = placemat_core::board::write(board).unwrap();
    std::fs::write(&path, text).unwrap();
    let pilot = Pilot::start().await;
    let mut args = launch_with("headless", &[&path]);
    args["scale"] = json!(2.0);
    args["settle_ms"] = json!(20_000);
    pilot.ok("launch", args).await;
    pilot
        .ok(
            "wait",
            json!({"target": {"id": ["board"]}, "timeout_ms": 30_000}),
        )
        .await;
    pilot
        .ok("screenshot", json!({"save_to": dir.path().join("fit.png")}))
        .await;
    let fit = pilot.ok("tree", json!({})).await;
    let (cx, cy) = centre(rect(node(&fit, "board")));
    // 1.1^60 is far past 30, so the zoom ends clamped at Camera::MAX_ZOOM.
    pilot
        .ok(
            "scroll",
            json!({"target": {"x": cx, "y": cy}, "notches": -60}),
        )
        .await;
    pilot
        .ok(
            "screenshot",
            json!({"save_to": dir.path().join("zoom-30.png")}),
        )
        .await;
    let tree = pilot.ok("tree", json!({})).await;
    assert_eq!(tree["settled"], json!(true), "{tree}");
    node(&tree, "board");
    fit
}

#[tokio::test(flavor = "multi_thread")]
async fn the_limits_board_paints_at_full_zoom_on_a_double_scale_display() {
    let board = board_util::limits_board();
    let fit = paint_at_fit_and_full_zoom(&board).await;
    // The budget leaves most labels unpainted at fit; the leaf still names the arrow.
    let first = board
        .paint_order()
        .find(|e| matches!(e.kind, placemat_core::board::Kind::Arrow(_)))
        .unwrap();
    let id = format!("shape-{}", first.id.as_str());
    let leaf = *leaves(&fit).iter().find(|n| segment(n) == id).unwrap();
    assert_eq!(leaf["name"], json!("a".repeat(105)), "{leaf}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stacked_hachure_board_paints_at_fit_and_full_zoom() {
    paint_at_fit_and_full_zoom(&board_util::stacked_hachure_board()).await;
}
