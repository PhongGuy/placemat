//! Culling to canvas items and the accessibility leaf each item publishes.

use std::path::Path;

use gpui_kit::accesskit::{self, Role};
use placemat_app::viewer::frame::{
    canvas_info, cull, leaf_bounds, leaf_id, leaf_name, leaf_role, leaves,
};
use placemat_core::board::{Board, ElementId, load, read_file};
use placemat_core::scene::{Camera, Detail, FixedAdvance, Rect, Scene};

fn canonical(name: &str) -> Board {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../placemat-core/tests/fixtures/boards/canonical")
        .join(name);
    read_file(&path).unwrap().board
}

fn el<'a>(b: &'a Board, id: &str) -> &'a placemat_core::board::Element {
    b.get(&ElementId::new(id).unwrap()).unwrap()
}

#[test]
fn cull_maps_visible_bounds_through_the_camera_in_paint_order() {
    let board = canonical("rect.board");
    let scene = Scene::build(&board, &FixedAdvance::new(0.5, 1.25));
    let camera = Camera::new((50.0, 100.0), 2.0);
    let items = cull(&scene, camera, (2000.0, 1000.0));
    let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["3kTq8wZp", "7hWc4nRd"]);
    let world = scene
        .visible(Rect::new(-1e9, -1e9, 1e9, 1e9))
        .next()
        .unwrap()
        .bounds;
    let s = items[0].screen;
    assert_eq!(
        (s.x0, s.y0),
        ((world.x0 - 50.0) * 2.0, (world.y0 - 100.0) * 2.0)
    );
    assert_eq!(
        (s.x1, s.y1),
        ((world.x1 - 50.0) * 2.0, (world.y1 - 100.0) * 2.0)
    );
    assert_eq!(items[0].detail, Detail::Full);
    assert!(items[0].rank < items[1].rank);
}

#[test]
fn cull_drops_what_the_viewport_misses_and_simplifies_small_shapes() {
    let board = canonical("rect.board");
    let scene = Scene::build(&board, &FixedAdvance::new(0.5, 1.25));
    let left = cull(&scene, Camera::new((0.0, 0.0), 1.0), (400.0, 400.0));
    assert_eq!(
        left.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        ["3kTq8wZp"]
    );
    let far = cull(&scene, Camera::new((0.0, 0.0), 0.1), (400.0, 400.0));
    assert_eq!(far.len(), 2);
    assert!(
        far.iter().all(|i| i.detail == Detail::Simple),
        "200 units at 0.1 is 20 px"
    );
}

#[test]
fn leaves_take_the_role_for_their_kind() {
    let arrows = canonical("arrow.board");
    assert_eq!(leaf_role(&el(&arrows, "3kTq8wZp").kind), Role::Group);
    assert_eq!(leaf_role(&el(&arrows, "9rLm2xQe").kind), Role::Image);
    assert_eq!(
        leaf_role(&el(&canonical("ellipse.board"), "b2Dy6sKu").kind),
        Role::Group
    );
    assert_eq!(
        leaf_role(&el(&canonical("diamond.board"), "d1Mn5pQr").kind),
        Role::Group
    );
    assert_eq!(
        leaf_role(&el(&canonical("text.board"), "q1Nf3jTb").kind),
        Role::Label
    );
    assert_eq!(
        leaf_role(&el(&canonical("line.board"), "e5Gp0vXa").kind),
        Role::Image
    );
    assert_eq!(
        leaf_role(&el(&canonical("freedraw.board"), "f7Hj2kLm").kind),
        Role::Image
    );
}

#[test]
fn leaves_are_named_by_label_or_content_else_by_kind_and_id() {
    let arrows = canonical("arrow.board");
    assert_eq!(leaf_name(el(&arrows, "3kTq8wZp")), "Web client");
    assert_eq!(leaf_name(el(&arrows, "7hWc4nRd")), "Rect 7hWc4nRd");
    assert_eq!(leaf_name(el(&arrows, "9rLm2xQe")), "HTTPS");
    assert_eq!(leaf_name(el(&arrows, "aFree001")), "Arrow aFree001");
    assert_eq!(
        leaf_name(el(&canonical("diamond.board"), "d1Mn5pQr")),
        "Diamond d1Mn5pQr",
        "an empty label is none"
    );
    assert_eq!(
        leaf_name(el(&canonical("text.board"), "q1Nf3jTb")),
        "Sketch of the v1 request path."
    );
    assert_eq!(
        leaf_name(el(&canonical("freedraw.board"), "f7Hj2kLm")),
        "Freedraw f7Hj2kLm"
    );
    assert_eq!(
        leaf_id(&ElementId::new("q1Nf3jTb").unwrap()),
        "shape-q1Nf3jTb"
    );
}

#[test]
fn a_long_label_names_its_leaf_by_its_first_256_chars() {
    let mut board = canonical("rect.board");
    let id = ElementId::new("3kTq8wZp").unwrap();
    board.set_label(&id, Some("é".repeat(300))).unwrap();
    assert_eq!(leaf_name(board.get(&id).unwrap()), "é".repeat(256));
}

/// Three overlapping rects whose paint order (z) is the reverse of their top-to-bottom order.
const STACKED: &str = r##"placemat-board version=0

rect "low" {
    at 0 100
    seed 1
    size 200 100
    fill none
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    z "a1"
}

rect "high" {
    at 50 0
    seed 2
    size 200 100
    fill none
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    label "Top"
    z "a2"
}

rect "mid" {
    at 25 50
    seed 3
    size 200 100
    fill none
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    z "a3"
}
"##;

#[test]
fn leaves_come_in_paint_order_not_screen_order() {
    let board = load(STACKED.as_bytes()).unwrap().board;
    let scene = Scene::build(&board, &FixedAdvance::new(0.5, 1.25));
    let items = cull(&scene, Camera::new((0.0, 0.0), 1.0), (800.0, 600.0));
    let canvas = Rect::new(10.0, 20.0, 810.0, 620.0);
    let found = leaves(&items, &board, canvas, 2.0);
    let authors: Vec<&str> = found.iter().map(|l| l.author.as_str()).collect();
    assert_eq!(
        authors,
        ["shape-low", "shape-high", "shape-mid"],
        "z order, not top order"
    );
    assert_eq!(found[1].name, "Top");
    assert_eq!(found[0].name, "Rect low");
    assert!(found.iter().all(|l| l.role == Role::Group));
    let low = items[0].screen;
    assert_eq!(Some(found[0].bounds), leaf_bounds(low, canvas, 2.0));
}

#[test]
fn leaves_skip_items_wholly_off_the_canvas() {
    let board = load(STACKED.as_bytes()).unwrap().board;
    let scene = Scene::build(&board, &FixedAdvance::new(0.5, 1.25));
    let items = cull(&scene, Camera::new((0.0, 0.0), 1.0), (800.0, 600.0));
    let narrow = Rect::new(0.0, 0.0, 800.0, 40.0);
    let found = leaves(&items, &board, narrow, 1.0);
    assert_eq!(
        found.iter().map(|l| l.author.as_str()).collect::<Vec<_>>(),
        ["shape-high"]
    );
}

#[test]
fn leaf_bounds_move_onto_the_window_clip_to_the_canvas_and_scale() {
    let canvas = Rect::new(100.0, 40.0, 500.0, 340.0);
    let inside = leaf_bounds(Rect::new(10.0, 10.0, 50.0, 30.0), canvas, 2.0).unwrap();
    assert_eq!(
        (inside.x0, inside.y0, inside.x1, inside.y1),
        (220.0, 100.0, 300.0, 140.0)
    );
    let edge = leaf_bounds(Rect::new(-20.0, -20.0, 10.0, 10.0), canvas, 2.0).unwrap();
    assert_eq!(
        edge,
        accesskit::Rect {
            x0: 200.0,
            y0: 80.0,
            x1: 220.0,
            y1: 100.0
        }
    );
    assert_eq!(
        leaf_bounds(Rect::new(-50.0, 0.0, -10.0, 10.0), canvas, 1.0),
        None
    );
}

#[test]
fn the_canvas_node_is_named_board_canvas_for_session_trees() {
    let mut node = accesskit::Node::new(Role::Canvas);
    canvas_info(&mut node);
    assert_eq!(node.author_id(), Some("board-canvas"));
    assert_eq!(node.label(), Some("Board"));
}

#[test]
fn a_leaf_name_shows_hidden_characters_as_tags() {
    let mut board = canonical("rect.board");
    let id = ElementId::new("3kTq8wZp").unwrap();
    let mut name = |label: &str| {
        board.set_label(&id, Some(label.to_owned())).unwrap();
        leaf_name(board.get(&id).unwrap())
    };
    assert_eq!(name("report\u{202E}txt.exe"), "report<U+202E>txt.exe");
    assert_eq!(name("a\tb\r\nsecond"), "a b");
    assert_eq!(name("a\rb"), "a<U+000D>b");
    assert_eq!(name("\u{645}\u{631}\u{200F}"), "\u{645}\u{631}\u{200F}");
    assert_eq!(
        name("\u{202E}".repeat(300).as_str()),
        "<U+202E>".repeat(256),
        "cap the source, then expand"
    );
}
