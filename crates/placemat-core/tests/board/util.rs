//! Builders shared by the board tests.

use placemat_core::board::{
    Boxed, Element, ElementId, Kind, LimitReached, Point, Seed, Size, ZKey,
};

pub fn id(s: &str) -> ElementId {
    ElementId::new(s).unwrap()
}

pub fn z(s: &str) -> ZKey {
    ZKey::new(s).unwrap()
}

pub fn seed(n: u32) -> Seed {
    Seed::new(n).unwrap()
}

pub fn el(i: &str, s: u32, k: &str, kind: Kind) -> Element {
    Element::new(id(i), seed(s), z(k), kind)
}

pub fn rect(i: &str, x: i32, y: i32) -> Element {
    el(
        i,
        1,
        "a1",
        Kind::Rect(Boxed::new(Point::new(x, y), Size::new(200, 100))),
    )
}

pub fn labelled(i: &str, label: String) -> Element {
    let mut boxed = Boxed::new(Point::new(0, 0), Size::new(1, 1));
    boxed.label = Some(label);
    el(i, 1, "a1", Kind::Rect(boxed))
}

/// `LimitReached` is non-exhaustive, so tests compare the limit's name.
pub fn limit(r: Result<(), LimitReached>) -> Result<(), &'static str> {
    r.map_err(|e| e.limit)
}
