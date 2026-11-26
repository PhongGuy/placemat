//! The pure half of painting: colours, widths, the world-to-canvas map and the sets per detail.

use std::path::Path;

use gpui_kit::Rgba;
use placemat_app::viewer::paint::{painted, rgba, screen_len, sketch_px, stroke_px, to_px};
use placemat_core::board::{Colour, read_file};
use placemat_core::scene::{Camera, Detail, FixedAdvance, Op, Rect, Scene, SetKind};

#[test]
fn colours_scale_each_channel_to_one() {
    assert_eq!(
        rgba(Colour::rgba(0x1e, 0, 0xff, 0x80)),
        Rgba {
            r: 30.0 / 255.0,
            g: 0.0,
            b: 1.0,
            a: 128.0 / 255.0
        }
    );
    assert_eq!(rgba(Colour::rgb(0, 0, 0)).a, 1.0);
}

#[test]
fn strokes_scale_with_zoom_but_never_vanish() {
    assert_eq!(stroke_px(20, 1.5), 3.0);
    assert_eq!(stroke_px(0, 30.0), 1.0);
    assert_eq!(stroke_px(20, 0.1), 1.0);
    assert_eq!(sketch_px(20, 1.5), 1.5);
    assert_eq!(sketch_px(20, 0.1), 0.5);
}

#[test]
fn local_points_land_on_the_canvas_through_the_camera() {
    let camera = Camera::new((0.0, 10.0), 2.0);
    assert_eq!(to_px((5.0, 5.0), (10.0, 20.0), camera), (30.0, 30.0));
    let far = Camera::new((16_777_216.0, 0.0), 30.0);
    assert_eq!(
        to_px((0.5, 0.0), (16_777_216.0, 0.0), far),
        (15.0, 0.0),
        "f64 keeps the half unit"
    );
}

#[test]
fn screen_length_sums_the_control_polygon() {
    let camera = Camera::new((0.0, 0.0), 2.0);
    let ops = [Op::Move(0.0, 0.0), Op::Line(3.0, 4.0), Op::Line(3.0, 8.0)];
    assert_eq!(screen_len(&ops, (0.0, 0.0), camera), 18.0);
    let curve = [
        Op::Move(0.0, 0.0),
        Op::Cubic([0.0, 0.0, 0.0, 0.0, 3.0, 4.0]),
    ];
    assert_eq!(
        screen_len(&curve, (0.0, 0.0), Camera::new((0.0, 0.0), 1.0)),
        5.0
    );
    let two = [
        Op::Move(0.0, 0.0),
        Op::Line(1.0, 0.0),
        Op::Move(10.0, 0.0),
        Op::Line(11.0, 0.0),
    ];
    assert_eq!(
        screen_len(&two, (0.0, 0.0), Camera::new((0.0, 0.0), 1.0)),
        2.0,
        "a Move does not count"
    );
}

#[test]
fn each_detail_paints_its_own_sets() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../placemat-core/tests/fixtures/boards/canonical/rect.board");
    let board = read_file(&path).unwrap().board;
    let scene = Scene::build(&board, &FixedAdvance::new(0.5, 1.25));
    let hachured = scene
        .visible(Rect::new(-1e9, -1e9, 1e9, 1e9))
        .next()
        .unwrap()
        .geometry;
    let full: Vec<SetKind> = painted(hachured, Detail::Full).map(|s| s.kind).collect();
    assert!(
        full.contains(&SetKind::Sketch) && full.contains(&SetKind::Stroke),
        "{full:?}"
    );
    let simple: Vec<SetKind> = painted(hachured, Detail::Simple).map(|s| s.kind).collect();
    assert!(
        !simple.is_empty() && simple.iter().all(|k| *k == SetKind::Stroke),
        "{simple:?}"
    );
    assert_eq!(painted(hachured, Detail::Dot).count(), 0);
}
