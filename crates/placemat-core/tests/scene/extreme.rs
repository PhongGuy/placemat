//! Hostile but valid boards: no panic, ops under the caps, and the box fallback never taken.

use placemat_core::board::Board;
use placemat_core::scene::Scene;

use crate::board_util::limits_board;
use crate::boardgen;
use crate::util::{STUB, everything, extremes_board};

/// Ops one element may hold, outline included; the measured worst case is about 50,000.
const OPS_MAX: usize = 60_000;

fn check(name: &str, board: &Board) {
    let scene = Scene::build(board, &STUB);
    assert_eq!(
        scene.visible(everything()).count(),
        board.len(),
        "{name}: an element left the scene"
    );
    for v in scene.visible(everything()) {
        let g = v.geometry;
        let ops: usize = g.rough.iter().chain(&g.outline).map(|s| s.ops.len()).sum();
        assert!(!g.fallback, "{name} {}: the box fallback was taken", v.id);
        assert!(ops <= OPS_MAX, "{name} {}: {ops} ops", v.id);
        let b = v.bounds;
        let all = [b.x0, b.y0, b.x1, b.y1, v.origin.0, v.origin.1];
        assert!(all.iter().all(|x| x.is_finite()), "{name} {}: {b:?}", v.id);
    }
}

#[test]
fn the_limits_board_builds_within_the_caps() {
    check("limits", &limits_board());
}

#[test]
fn every_kind_at_the_edge_of_its_range_builds_within_the_caps() {
    check("extremes", &extremes_board());
}

#[test]
fn generated_boards_build_within_the_caps() {
    for s in 0..32 {
        check(
            &format!("seed {s}"),
            &boardgen::board(&mut fastrand::Rng::with_seed(s)),
        );
    }
}
