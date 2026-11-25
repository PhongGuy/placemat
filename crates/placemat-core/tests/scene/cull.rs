//! The flat cull scan, paint order and the invalidate choke point.

use placemat_core::board::{ArrowEnd, Board, ElementId, Kind};
use placemat_core::scene::{Rect, Scene};

use crate::util::{Row, STUB, arrow, everything, id, rect, rows, small_board, z};

fn order(rows: &[Row]) -> Vec<(&str, u32)> {
    rows.iter().map(|r| (r.0.as_str(), r.1)).collect()
}

#[test]
fn visible_returns_the_intersecting_elements_in_paint_order() {
    let mut board = Board::new();
    for e in [
        rect("a", "a3", 0, 0),
        rect("b", "a1", 1000, 0),
        rect("c", "a2", 0, 0),
    ] {
        board.insert(e).unwrap();
    }
    let scene = Scene::build(&board, &STUB);
    assert_eq!(
        order(&rows(&scene, everything())),
        [("b", 0), ("c", 1), ("a", 2)]
    );
    assert_eq!(
        order(&rows(&scene, Rect::new(0.0, 0.0, 300.0, 150.0))),
        [("c", 1), ("a", 2)]
    );
    assert!(
        rows(&scene, Rect::new(5000.0, 5000.0, 6000.0, 6000.0)).is_empty(),
        "far view not empty"
    );
}

#[test]
fn equal_z_keys_paint_in_id_order() {
    let mut board = Board::new();
    board.insert(rect("m", "a1", 0, 0)).unwrap();
    board.insert(rect("k", "a1", 0, 0)).unwrap();
    assert_eq!(
        order(&rows(&Scene::build(&board, &STUB), everything())),
        [("k", 0), ("m", 1)]
    );
}

#[test]
fn bounds_is_the_union_or_none_when_empty() {
    assert_eq!(Scene::build(&Board::new(), &STUB).bounds(), None);
    let mut board = Board::new();
    board.insert(rect("a", "a1", 0, 0)).unwrap();
    board.insert(rect("b", "a1", 1000, 500)).unwrap();
    let b = Scene::build(&board, &STUB).bounds().unwrap();
    assert!(
        b.x0 <= 0.0 && b.y0 <= 0.0 && b.x1 >= 1200.0 && b.y1 >= 600.0,
        "{b:?}"
    );
}

#[test]
fn moving_a_target_regenerates_its_bound_arrows() {
    let mut board = Board::new();
    for e in [
        rect("a", "a1", 0, 0),
        rect("b", "a1", 400, 0),
        arrow("x", "a2", "a", "b"),
    ] {
        board.insert(e).unwrap();
    }
    let mut scene = Scene::build(&board, &STUB);
    let before = rows(&scene, everything());
    board.insert(rect("b", "a1", 400, 300)).unwrap();
    scene.invalidate(&board, &STUB, &id("b"));
    let after = rows(&scene, everything());
    assert_eq!(after, rows(&Scene::build(&board, &STUB), everything()));
    let arrow_bounds = |rows: &[Row]| rows.iter().find(|r| r.0 == "x").unwrap().2;
    assert_ne!(
        arrow_bounds(&before),
        arrow_bounds(&after),
        "the arrow kept its old ends"
    );
}

#[test]
fn invalidate_adds_removes_and_reranks() {
    let mut board = Board::new();
    for e in [rect("a", "a1", 0, 0), rect("b", "a2", 0, 0)] {
        board.insert(e).unwrap();
    }
    let mut scene = Scene::build(&board, &STUB);
    let mut a = board.get(&id("a")).unwrap().clone();
    a.z = z("a3");
    board.insert(a).unwrap();
    scene.invalidate(&board, &STUB, &id("a"));
    assert_eq!(order(&rows(&scene, everything())), [("b", 0), ("a", 1)]);
    board.remove(&id("b"));
    scene.invalidate(&board, &STUB, &id("b"));
    board.insert(rect("c", "a01", 0, 0)).unwrap();
    scene.invalidate(&board, &STUB, &id("c"));
    assert_eq!(order(&rows(&scene, everything())), [("c", 0), ("a", 1)]);
}

fn shift(kind: &mut Kind, dx: i32, dy: i32) {
    let p = match kind {
        Kind::Rect(b) | Kind::Ellipse(b) | Kind::Diamond(b) => &mut b.at,
        Kind::Line(p) | Kind::Freedraw(p) => &mut p.at,
        Kind::Text(t) => &mut t.at,
        Kind::Arrow(a) => {
            let ArrowEnd::Free(p) = &mut a.from else {
                return;
            };
            p
        }
        _ => return,
    };
    (p.x, p.y) = (p.x + dx, p.y + dy);
}

/// One random edit; returns the id to invalidate.
fn edit(b: &mut Board, r: &mut fastrand::Rng) -> ElementId {
    let ids: Vec<ElementId> = b.elements().map(|e| e.id.clone()).collect();
    if ids.is_empty() || r.u8(..4) == 0 {
        let new = format!("n{}", r.u32(..));
        b.insert(rect(&new, "a2", r.i32(-2000..=2000), 0)).unwrap();
        return id(&new);
    }
    let target = ids[r.usize(..ids.len())].clone();
    let mut e = b.get(&target).unwrap().clone();
    match r.u8(..3) {
        0 => drop(b.remove(&target)),
        1 => {
            e.z = z(["a01", "a2", "a4"][r.usize(..3)]);
            b.insert(e).unwrap();
        }
        _ => {
            shift(&mut e.kind, r.i32(-500..=500), r.i32(-500..=500));
            b.insert(e).unwrap();
        }
    }
    target
}

#[test]
fn invalidate_after_any_edit_matches_a_fresh_build() {
    for s in 0..50 {
        let mut r = fastrand::Rng::with_seed(s);
        let mut board = small_board(&mut r);
        let mut scene = Scene::build(&board, &STUB);
        for step in 0..8 {
            let changed = edit(&mut board, &mut r);
            scene.invalidate(&board, &STUB, &changed);
            let fresh = rows(&Scene::build(&board, &STUB), everything());
            assert_eq!(rows(&scene, everything()), fresh, "seed {s} step {step}");
        }
    }
}
