//! The board's mutators and the one limit set they enforce.

use placemat_core::board::{
    Anchor, Arrow, ArrowEnd, Board, Kind, LIMITS, Path, Point, Text, VERSION,
};

use crate::util::{el, id, labelled, limit, rect};

fn pts(n: usize) -> Vec<Point> {
    vec![Point::new(1, 1); n]
}

fn freedraw(i: &str, n: usize) -> placemat_core::board::Element {
    el(
        i,
        1,
        "a1",
        Kind::Freedraw(Path::new(Point::new(0, 0), pts(n))),
    )
}

#[test]
fn limits_match_the_spec() {
    assert_eq!(LIMITS.hard_bytes, 16 << 20);
    assert_eq!(LIMITS.team_bytes, 1 << 20);
    assert_eq!(LIMITS.elements, 10_000);
    assert_eq!(LIMITS.points_total, 250_000);
    assert_eq!(LIMITS.points_per_element, 10_000);
    assert_eq!(LIMITS.string_bytes_total, 1 << 20);
    assert_eq!(LIMITS.string_bytes_each, 64 << 10);
    assert_eq!(LIMITS.unknown_bytes_total, 1 << 20);
    assert_eq!(VERSION, 0);
}

#[test]
fn insert_replaces_by_id_and_iterates_in_id_order() {
    let mut b = Board::new();
    assert_eq!(b.version(), VERSION);
    assert!(b.is_empty());
    for (i, x) in [("b", 1), ("a", 2), ("B", 3)] {
        b.insert(rect(i, x, 0)).unwrap();
    }
    b.insert(rect("a", 9, 0)).unwrap();
    let ids: Vec<&str> = b.elements().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["B", "a", "b"]);
    assert_eq!(b.len(), 3);
    assert!(matches!(&b.get(&id("a")).unwrap().kind, Kind::Rect(r) if r.at == Point::new(9, 0)));
    assert!(b.contains_id("B") && !b.contains_id("c"));
    assert_eq!(b.remove(&id("a")).map(|e| e.id), Some(id("a")));
    assert_eq!(b.remove(&id("a")), None);
    assert_eq!(b.len(), 2);
}

#[test]
fn insert_refuses_the_element_past_the_limit() {
    let mut b = Board::new();
    for i in 0..LIMITS.elements {
        b.insert(rect(&format!("e{i}"), 0, 0)).unwrap();
    }
    assert_eq!(limit(b.insert(rect("over", 0, 0))), Err("elements"));
    assert_eq!(b.len(), LIMITS.elements);
    b.insert(rect("e0", 5, 5)).unwrap();
}

#[test]
fn points_are_capped_per_element_and_in_total() {
    let mut b = Board::new();
    assert_eq!(
        limit(b.insert(freedraw("big", LIMITS.points_per_element + 1))),
        Err("points_per_element")
    );
    for i in 0..LIMITS.points_total / LIMITS.points_per_element {
        b.insert(freedraw(&format!("f{i}"), LIMITS.points_per_element))
            .unwrap();
    }
    assert_eq!(limit(b.insert(freedraw("one", 1))), Err("points_total"));
    assert_eq!(
        limit(b.set_points(&id("f0"), pts(LIMITS.points_per_element + 1))),
        Err("points_per_element")
    );
    b.set_points(&id("f0"), pts(10)).unwrap();
    b.insert(freedraw("one", 1)).unwrap();
    assert_eq!(
        limit(b.set_points(&id("one"), pts(LIMITS.points_per_element))),
        Err("points_total")
    );
    assert!(matches!(&b.get(&id("one")).unwrap().kind, Kind::Freedraw(p) if p.points.len() == 1));
}

#[test]
fn strings_are_capped_after_escaping() {
    let each = LIMITS.string_bytes_each;
    let mut b = Board::new();
    b.insert(labelled("a", "a".repeat(each))).unwrap();
    assert_eq!(
        limit(b.insert(labelled("b", "a".repeat(each + 1)))),
        Err("string_bytes_each")
    );
    // A quote escapes to two bytes and U+202E to eight (`\u{202e}`).
    assert_eq!(
        limit(b.insert(labelled("c", "\"".repeat(each / 2 + 1)))),
        Err("string_bytes_each")
    );
    assert_eq!(
        limit(b.insert(labelled("d", "\u{202e}".repeat(each / 8 + 1)))),
        Err("string_bytes_each")
    );
    assert_eq!(
        limit(b.set_label(&id("a"), Some("a".repeat(each + 1)))),
        Err("string_bytes_each")
    );
    b.insert(el(
        "t",
        1,
        "a1",
        Kind::Text(Text::new(Point::new(0, 0), String::new())),
    ))
    .unwrap();
    assert_eq!(
        limit(b.set_content(&id("t"), "a".repeat(each + 1))),
        Err("string_bytes_each")
    );
    b.set_content(&id("t"), "ok".to_owned()).unwrap();
}

#[test]
fn strings_are_capped_in_total() {
    let mut b = Board::new();
    for i in 0..LIMITS.string_bytes_total / LIMITS.string_bytes_each {
        b.insert(labelled(
            &format!("l{i}"),
            "a".repeat(LIMITS.string_bytes_each),
        ))
        .unwrap();
    }
    assert_eq!(
        limit(b.insert(labelled("over", "a".to_owned()))),
        Err("string_bytes_total")
    );
    b.set_label(&id("l0"), None).unwrap();
    b.insert(labelled("over", "a".to_owned())).unwrap();
}

#[test]
fn values_outside_their_formats_are_refused() {
    let max = 1 << 24;
    let mut b = Board::new();
    b.insert(rect("ok", max, -max)).unwrap();
    assert_eq!(limit(b.insert(rect("x", max + 1, 0))), Err("range"));
    let mut wide = rect("w", 0, 0);
    wide.stroke.width = 1001;
    assert_eq!(limit(b.insert(wide)), Err("range"));
    let mut rough = rect("r", 0, 0);
    rough.stroke.roughness = 101;
    assert_eq!(limit(b.insert(rough)), Err("range"));
    let arrow = |x| {
        let from = ArrowEnd::Bound {
            target: id("ok"),
            anchor: Anchor::new(x, 0),
        };
        el(
            "ar",
            1,
            "a1",
            Kind::Arrow(Arrow::new(from, ArrowEnd::Free(Point::new(0, 0)))),
        )
    };
    b.insert(arrow(10_000)).unwrap();
    assert_eq!(limit(b.insert(arrow(10_001))), Err("range"));
    let tenths = 10 * max;
    let line = |p| {
        el(
            "p",
            1,
            "a1",
            Kind::Line(Path::new(Point::new(0, 0), vec![p])),
        )
    };
    b.insert(line(Point::new(tenths, -tenths))).unwrap();
    assert_eq!(
        limit(b.insert(line(Point::new(tenths + 1, 0)))),
        Err("range")
    );
    assert_eq!(
        limit(b.set_points(&id("p"), vec![Point::new(0, -tenths - 1)])),
        Err("range")
    );
    let mut text = Text::new(Point::new(0, 0), String::new());
    text.font.size = 1001;
    assert_eq!(
        limit(b.insert(el("t", 1, "a1", Kind::Text(text)))),
        Err("range")
    );
}

#[test]
fn setters_on_a_missing_element_or_field_change_nothing() {
    let mut b = Board::new();
    b.insert(rect("a", 0, 0)).unwrap();
    let before = b.clone();
    b.set_label(&id("nope"), Some("x".to_owned())).unwrap();
    b.set_content(&id("a"), "x".to_owned()).unwrap();
    b.set_points(&id("a"), Vec::new()).unwrap();
    assert_eq!(b, before);
}
