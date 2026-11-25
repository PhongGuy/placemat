//! Builders shared by the scene tests.

use placemat_core::board::{
    Anchor, Arrow, ArrowEnd, Board, Boxed, Element, ElementId, Kind, Point, Seed, Size, Text, ZKey,
};
use placemat_core::scene::{FixedAdvance, Geometry, Rect, Scene};

pub const STUB: FixedAdvance = FixedAdvance::new(0.5, 1.25);

pub type Row = (String, u32, Rect, (f64, f64), Geometry);

pub fn everything() -> Rect {
    Rect::new(-1e12, -1e12, 1e12, 1e12)
}

pub fn id(s: &str) -> ElementId {
    ElementId::new(s).unwrap()
}

pub fn z(s: &str) -> ZKey {
    ZKey::new(s).unwrap()
}

pub fn rect(i: &str, z_key: &str, x: i32, y: i32) -> Element {
    let body = Boxed::new(Point::new(x, y), Size::new(200, 100));
    Element::new(id(i), Seed::new(1).unwrap(), z(z_key), Kind::Rect(body))
}

pub fn arrow(i: &str, z_key: &str, from: &str, to: &str) -> Element {
    let end = |t: &str, x| ArrowEnd::Bound {
        target: id(t),
        anchor: Anchor::new(x, 5_000),
    };
    let a = Arrow::new(end(from, 10_000), end(to, 0));
    Element::new(id(i), Seed::new(2).unwrap(), z(z_key), Kind::Arrow(a))
}

pub fn rows(scene: &Scene, view: Rect) -> Vec<Row> {
    scene
        .visible(view)
        .map(|v| {
            (
                v.id.to_string(),
                v.rank,
                v.bounds,
                v.origin,
                v.geometry.clone(),
            )
        })
        .collect()
}

/// A small board of every kind near the origin, whose arrows bind to any earlier id or themselves.
pub fn small_board(r: &mut fastrand::Rng) -> Board {
    let mut b = Board::new();
    for i in 0..r.usize(4..30) {
        let name = format!("e{i}");
        let (x, y) = (r.i32(-2000..=2000), r.i32(-2000..=2000));
        let kind = match r.u8(..4) {
            0 => Kind::Ellipse(Boxed::new(
                Point::new(x, y),
                Size::new(r.i32(-300..=300), 80),
            )),
            1 => Kind::Text(Text::new(Point::new(x, y), format!("t{i}\nline"))),
            2 => {
                let mut end = || {
                    if r.bool() {
                        let target = id(&format!("e{}", r.usize(..=i)));
                        ArrowEnd::Bound {
                            target,
                            anchor: Anchor::new(r.u16(..=10_000), 0),
                        }
                    } else {
                        ArrowEnd::Free(Point::new(r.i32(-2000..=2000), 0))
                    }
                };
                Kind::Arrow(Arrow::new(end(), end()))
            }
            _ => Kind::Rect(Boxed::new(
                Point::new(x, y),
                Size::new(120, r.i32(-90..=90)),
            )),
        };
        let e = Element::new(
            id(&name),
            Seed::new(r.u32(..=Seed::MAX)).unwrap(),
            z(["a1", "a2", "a3"][r.usize(..3)]),
            kind,
        );
        b.insert(e).unwrap();
    }
    b
}
