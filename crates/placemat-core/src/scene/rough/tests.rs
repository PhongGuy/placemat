//! roughr behind Placemat's op type: determinism, local coordinates, degenerate shapes, the caps.

use super::{Built, build, checked, guarded, hachure_gap};
use crate::board::{
    Anchor, Arrow, ArrowEnd, Board, Boxed, Colour, Element, ElementId, Fill, FillStyle, Head, Kind,
    Path, Point, Seed, Size, Text, ZKey,
};
use crate::scene::{FixedAdvance, Geometry, Op, OpSet, Rect, SetKind};

const M: i32 = 1 << 24;
/// Ops one element may hold at the edge of every range, outline included.
const OPS_MAX: usize = 60_000;
const STUB: FixedAdvance = FixedAdvance::new(0.5, 1.25);
/// The plain closed 10 x 5 box.
const PLAIN: [Op; 5] = [
    Op::Move(0.0, 0.0),
    Op::Line(10.0, 0.0),
    Op::Line(10.0, 5.0),
    Op::Line(0.0, 5.0),
    Op::Line(0.0, 0.0),
];

fn el(seed: u32, kind: Kind) -> Element {
    let (id, z) = (ElementId::new("e").unwrap(), ZKey::new("a1").unwrap());
    Element::new(id, Seed::new(seed).unwrap(), z, kind)
}

fn boxed(at: (i32, i32), size: (i32, i32), style: Option<FillStyle>) -> Boxed {
    let mut b = Boxed::new(Point::new(at.0, at.1), Size::new(size.0, size.1));
    if let Some(style) = style {
        b.fill = Fill::Colour {
            colour: Colour::rgb(0, 0, 0),
            style,
        };
    }
    b
}

fn built(e: &Element) -> Built {
    build(e, &Board::new(), &STUB)
}

fn values(g: &Geometry) -> Vec<f32> {
    let sets = g.rough.iter().chain(&g.outline);
    sets.flat_map(|s| &s.ops)
        .flat_map(|op| match *op {
            Op::Move(x, y) | Op::Line(x, y) => vec![x, y],
            Op::Cubic(c) => c.to_vec(),
        })
        .collect()
}

fn op_count(g: &Geometry) -> usize {
    g.rough.iter().chain(&g.outline).map(|s| s.ops.len()).sum()
}

fn is_dot(sets: &[OpSet]) -> bool {
    matches!(sets, [s] if s.kind == SetKind::Stroke
        && matches!(s.ops.as_slice(), [Op::Move(..), Op::Cubic(_), Op::Cubic(_), Op::Cubic(_), Op::Cubic(_)]))
}

fn within_caps(g: &Geometry) -> bool {
    !g.fallback && op_count(g) <= OPS_MAX && values(g).iter().all(|v| v.is_finite())
}

#[test]
fn the_same_element_gives_the_same_ops_and_another_seed_does_not() {
    let kind = Kind::Rect(boxed((0, 0), (200, 100), Some(FillStyle::Hachure)));
    let a = built(&el(7, kind.clone()));
    assert_eq!(a.geometry, built(&el(7, kind.clone())).geometry);
    assert_ne!(a.geometry.rough, built(&el(8, kind)).geometry.rough);
}

#[test]
fn ops_are_local_to_the_normalised_box() {
    let b = built(&el(7, Kind::Ellipse(boxed((100, 50), (-200, -100), None))));
    assert_eq!(b.origin, (-100.0, -50.0));
    for v in values(&b.geometry) {
        assert!((-20.0..=220.0).contains(&v), "{v} is not local to the box");
    }
    let r = b.bounds;
    assert!(
        r.x0 <= -100.0 && r.y0 <= -50.0 && r.x1 >= 100.0 && r.y1 >= 50.0,
        "{r:?}"
    );
}

#[test]
fn a_rect_outline_is_its_plain_closed_box() {
    let g = built(&el(7, Kind::Rect(boxed((0, 0), (10, 5), None)))).geometry;
    assert!(
        matches!(g.outline.as_slice(), [s] if s.ops == PLAIN),
        "{g:?}"
    );
}

#[test]
fn a_flat_box_draws_one_unfilled_line() {
    let body = boxed((0, 0), (200, 0), Some(FillStyle::CrossHatch));
    for kind in [
        Kind::Rect(body.clone()),
        Kind::Ellipse(body.clone()),
        Kind::Diamond(body),
    ] {
        let g = built(&el(7, kind)).geometry;
        assert!(
            !g.fallback && g.rough.iter().all(|s| s.kind == SetKind::Stroke),
            "{g:?}"
        );
        assert!(
            matches!(g.outline.as_slice(), [s] if matches!(s.ops.as_slice(), [Op::Move(..), Op::Line(..)])),
            "{g:?}"
        );
    }
}

#[test]
fn a_zero_box_and_a_path_under_two_distinct_points_draw_dots() {
    let at = Point::new(5, 5);
    let kinds = [
        Kind::Rect(boxed((5, 5), (0, 0), Some(FillStyle::Hachure))),
        Kind::Line(Path::new(at, vec![])),
        Kind::Line(Path::new(at, vec![Point::new(10, 10); 3])),
        Kind::Freedraw(Path::new(at, vec![Point::new(10, 10)])),
    ];
    for kind in kinds {
        let g = built(&el(7, kind)).geometry;
        assert!(is_dot(&g.rough) && g.rough == g.outline, "{g:?}");
    }
}

#[test]
fn duplicate_consecutive_points_are_dropped() {
    let line = |pts: &[(i32, i32)]| {
        let points = pts.iter().map(|&(x, y)| Point::new(x, y)).collect();
        Kind::Line(Path::new(Point::new(0, 0), points))
    };
    let a = built(&el(
        7,
        line(&[(0, 0), (0, 0), (1000, 0), (1000, 0), (1000, 500)]),
    ));
    let b = built(&el(7, line(&[(0, 0), (1000, 0), (1000, 500)])));
    assert_eq!(a.geometry, b.geometry);
}

#[test]
fn the_hachure_gap_keeps_roughrs_default_and_caps_the_line_count() {
    assert_eq!(hachure_gap(2.0, 200.0), 8.0);
    assert_eq!(hachure_gap(0.0, 10.0), 0.1);
    assert_eq!(hachure_gap(0.0, 16_777_216.0), 16_777_216.0 / 2000.0);
}

#[test]
fn filled_boxes_at_the_limit_stay_under_the_op_cap() {
    let styles = [
        FillStyle::Hachure,
        FillStyle::CrossHatch,
        FillStyle::Solid,
        FillStyle::Zigzag,
    ];
    for style in styles {
        for (width, roughness) in [(0, 0), (1000, 100)] {
            for (at, size) in [((-M, -M), (M, M)), ((M, M), (-M, -1))] {
                let body = boxed(at, size, Some(style));
                for kind in [
                    Kind::Rect(body.clone()),
                    Kind::Ellipse(body.clone()),
                    Kind::Diamond(body),
                ] {
                    let mut e = el(7, kind);
                    (e.stroke.width, e.stroke.roughness) = (width, roughness);
                    let g = built(&e).geometry;
                    assert!(within_caps(&g), "{style:?} {size:?}: {} ops", op_count(&g));
                }
            }
        }
    }
}

#[test]
fn the_longest_paths_stay_under_the_op_cap() {
    let t = 10 * M;
    let mut rng = fastrand::Rng::with_seed(1);
    let points: Vec<Point> = (0..10_000)
        .map(|_| Point::new(rng.i32(-t..=t), rng.i32(-t..=t)))
        .collect();
    let kinds = [
        Kind::Line(Path::new(Point::new(M, -M), points.clone())),
        Kind::Freedraw(Path::new(Point::new(-M, M), points)),
    ];
    for kind in kinds {
        let mut e = el(7, kind);
        (e.stroke.width, e.stroke.roughness) = (1000, 100);
        let g = built(&e).geometry;
        assert!(within_caps(&g), "{} ops", op_count(&g));
    }
}

#[test]
fn non_finite_geometry_falls_back_to_the_plain_box() {
    let ops = vec![Op::Move(0.0, 0.0), Op::Line(f32::NAN, 1.0)];
    let bad = Geometry {
        rough: vec![OpSet {
            kind: SetKind::Stroke,
            ops,
        }],
        ..Geometry::default()
    };
    let g = checked(bad, 10.0, 5.0);
    assert!(g.fallback && g.rough == g.outline, "{g:?}");
    assert!(matches!(g.rough.as_slice(), [s] if s.ops == PLAIN), "{g:?}");
    let good = built(&el(7, Kind::Rect(boxed((0, 0), (10, 5), None)))).geometry;
    assert_eq!(checked(good.clone(), 10.0, 5.0), good);
}

#[test]
fn a_panic_in_generation_falls_back_to_the_plain_box() {
    let g = guarded(|| panic!("a roughr bug"), 10.0, 5.0);
    assert!(g.fallback && g.rough == g.outline, "{g:?}");
    assert!(matches!(g.rough.as_slice(), [s] if s.ops == PLAIN), "{g:?}");
    let good = built(&el(7, Kind::Rect(boxed((0, 0), (10, 5), None)))).geometry;
    assert_eq!(guarded(|| good.clone(), 10.0, 5.0), good);
}

fn text(at: (i32, i32), content: &str, size: u16) -> Kind {
    let mut t = Text::new(Point::new(at.0, at.1), content.to_owned());
    t.font.size = size;
    Kind::Text(t)
}

fn bound(target: &str, x: u16, y: u16) -> ArrowEnd {
    ArrowEnd::Bound {
        target: ElementId::new(target).unwrap(),
        anchor: Anchor::new(x, y),
    }
}

fn straight(from: ArrowEnd, to: ArrowEnd) -> Element {
    let mut e = el(7, Kind::Arrow(Arrow::new(from, to)));
    e.stroke.roughness = 0;
    e
}

fn shaft(dx: f32, dy: f32) -> Vec<OpSet> {
    vec![OpSet {
        kind: SetKind::Stroke,
        ops: vec![Op::Move(0.0, 0.0), Op::Line(dx, dy)],
    }]
}

fn named(id: &str, kind: Kind) -> Element {
    Element::new(
        ElementId::new(id).unwrap(),
        Seed::new(1).unwrap(),
        ZKey::new("a1").unwrap(),
        kind,
    )
}

#[test]
fn a_text_is_its_measured_block_with_no_ops() {
    let b = built(&el(7, text((10, 20), "abcd\nef", 200)));
    assert_eq!(b.origin, (10.0, 20.0));
    assert_eq!(b.bounds, Rect::new(10.0, 20.0, 50.0, 70.0));
    let label = b.geometry.label.clone().unwrap();
    assert_eq!(label.lines, ["abcd", "ef"]);
    assert_eq!((label.world_size, label.at), (20.0, (0.0, 0.0)));
    assert!(
        b.geometry.rough.is_empty() && b.geometry.outline.is_empty(),
        "{:?}",
        b.geometry
    );
    let zero = built(&el(7, text((10, 20), "abcd", 0)));
    assert_eq!(zero.bounds, Rect::new(10.0, 20.0, 10.0, 20.0));
}

#[test]
fn a_box_label_is_centred_and_an_empty_one_is_none() {
    let mut body = boxed((0, 0), (200, 100), None);
    body.label = Some("ab".to_owned());
    let label = built(&el(7, Kind::Rect(body.clone())))
        .geometry
        .label
        .unwrap();
    assert_eq!((label.world_size, label.at), (20.0, (90.0, 37.5)));
    body.label = Some(String::new());
    assert_eq!(built(&el(7, Kind::Rect(body))).geometry.label, None);
}

#[test]
fn bound_ends_sit_on_their_targets_at_the_anchor() {
    let mut board = Board::new();
    board
        .insert(named("a", Kind::Rect(boxed((0, 0), (200, 100), None))))
        .unwrap();
    board
        .insert(named("b", Kind::Rect(boxed((400, 0), (200, 100), None))))
        .unwrap();
    board
        .insert(named("t", text((1000, 0), "abcd", 200)))
        .unwrap();
    let ab = build(
        &straight(bound("a", 10_000, 5_000), bound("b", 0, 5_000)),
        &board,
        &STUB,
    );
    assert_eq!(
        (ab.origin, ab.geometry.outline),
        ((200.0, 50.0), shaft(200.0, 0.0))
    );
    // The text block spans 1000..1040 by 0..25.
    let at = build(
        &straight(bound("a", 0, 0), bound("t", 5_000, 10_000)),
        &board,
        &STUB,
    );
    assert_eq!(
        (at.origin, at.geometry.outline),
        ((0.0, 0.0), shaft(1020.0, 25.0))
    );
}

#[test]
fn a_missing_target_resolves_to_the_board_origin() {
    let e = straight(
        ArrowEnd::Free(Point::new(100, 100)),
        bound("gone", 5_000, 5_000),
    );
    let b = build(&e, &Board::new(), &STUB);
    assert_eq!(
        (b.origin, b.geometry.outline),
        ((100.0, 100.0), shaft(-100.0, -100.0))
    );
}

#[test]
fn each_head_adds_one_set_and_a_zero_length_arrow_is_a_dot() {
    for head in [Head::Arrow, Head::Triangle, Head::Bar, Head::Dot] {
        let mut a = Arrow::new(
            ArrowEnd::Free(Point::new(0, 0)),
            ArrowEnd::Free(Point::new(300, 0)),
        );
        (a.heads.start, a.heads.end) = (Head::None, head);
        assert_eq!(
            built(&el(7, Kind::Arrow(a.clone()))).geometry.rough.len(),
            2,
            "{head:?}"
        );
        a.heads.start = head;
        assert_eq!(
            built(&el(7, Kind::Arrow(a))).geometry.rough.len(),
            3,
            "{head:?}"
        );
    }
    let point = ArrowEnd::Free(Point::new(5, 5));
    let g = built(&el(7, Kind::Arrow(Arrow::new(point.clone(), point)))).geometry;
    assert!(is_dot(&g.rough), "{g:?}");
}

#[test]
fn an_arrow_label_is_centred_on_the_shaft() {
    let mut a = Arrow::new(
        ArrowEnd::Free(Point::new(0, 0)),
        ArrowEnd::Free(Point::new(300, 0)),
    );
    a.label = Some("ab".to_owned());
    let b = built(&el(7, Kind::Arrow(a)));
    assert_eq!(b.geometry.label.unwrap().at, (140.0, -12.5));
    assert!(
        b.bounds.y0 <= -12.5 && b.bounds.y1 >= 12.5,
        "{:?}",
        b.bounds
    );
}
