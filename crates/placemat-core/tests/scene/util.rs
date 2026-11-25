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

/// Every kind at the edge of its range: boxes reaching 2^25, every fill, 10,000-point paths
/// at the point limit, a 20-line text at size 100, and arrows between the far corners.
pub fn extremes_board() -> Board {
    use placemat_core::board::{Colour, Dash, Fill, FillStyle, Head, Path};
    const M: i32 = 1 << 24;
    let t = 10 * M;
    let fills = [
        None,
        Some(FillStyle::Hachure),
        Some(FillStyle::CrossHatch),
        Some(FillStyle::Solid),
        Some(FillStyle::Zigzag),
    ];
    let spans = [
        ((M, M), (M, M)),
        ((-M, -M), (-M, -M)),
        ((-M, -M), (M, M)),
        ((M, M), (-M, -M)),
        ((0, 0), (M, 1)),
        ((0, 0), (M, 0)),
        ((M, -M), (0, 0)),
    ];
    let mut kinds = Vec::new();
    for (at, size) in spans {
        for fill in fills {
            let mut body = Boxed::new(Point::new(at.0, at.1), Size::new(size.0, size.1));
            if let Some(style) = fill {
                body.fill = Fill::Colour {
                    colour: Colour::rgb(1, 2, 3),
                    style,
                };
            }
            body.label = Some("w".repeat(300));
            kinds.extend([
                Kind::Rect(body.clone()),
                Kind::Ellipse(body.clone()),
                Kind::Diamond(body),
            ]);
        }
    }
    let zigzag: Vec<Point> = (0..10_000)
        .map(|i| {
            if i % 2 == 0 {
                Point::new(-t, t)
            } else {
                Point::new(t, -t)
            }
        })
        .collect();
    kinds.push(Kind::Line(Path::new(Point::new(M, -M), zigzag.clone())));
    kinds.push(Kind::Freedraw(Path::new(Point::new(-M, M), zigzag)));
    let mut text = Text::new(Point::new(M, M), vec!["é".repeat(300); 20].join("\n"));
    text.font.size = 1000;
    kinds.push(Kind::Text(text));
    let mut free = Arrow::new(
        ArrowEnd::Free(Point::new(-M, -M)),
        ArrowEnd::Free(Point::new(M, M)),
    );
    (free.heads.start, free.heads.end) = (Head::Triangle, Head::Dot);
    free.label = Some("w".repeat(300));
    kinds.push(Kind::Arrow(free));
    // x0 is the box at (M, M) sized (M, M); x15 the one at (-M, -M) sized (-M, -M).
    let far = |target: &str| ArrowEnd::Bound {
        target: id(target),
        anchor: Anchor::new(10_000, 10_000),
    };
    kinds.push(Kind::Arrow(Arrow::new(far("x0"), far("x15"))));
    let mut b = Board::new();
    for (i, kind) in kinds.into_iter().enumerate() {
        let mut e = Element::new(
            id(&format!("x{i}")),
            Seed::new(Seed::MAX).unwrap(),
            z("a1"),
            kind,
        );
        (e.stroke.width, e.stroke.roughness, e.stroke.dash) = (1000, 100, Dash::Dotted);
        b.insert(e).unwrap();
    }
    b
}
