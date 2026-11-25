//! A seeded generator of boards spanning every kind, format range and escape.

use fastrand::Rng as Fast;
use placemat_core::board::{
    Align, Anchor, Arrow, ArrowEnd, Board, Boxed, Colour, Dash, Element, ElementId, Fill,
    FillStyle, FontFamily, Head, Kind, Path, Point, Rng, Size, Text, ZKey,
};

const MAX: i32 = 1 << 24;
const NASTY: [&str; 13] = [
    "a", " ", "\"", "\\", "\n", "\r", "\t", "\u{0}", "\u{85}", "\u{202e}", "\u{feff}", "é", "🎉",
];

fn string(r: &mut Fast) -> String {
    (0..r.usize(0..12))
        .map(|_| NASTY[r.usize(..NASTY.len())])
        .collect()
}

fn colour(r: &mut Fast) -> Colour {
    let (red, green, blue) = (r.u8(..), r.u8(..), r.u8(..));
    if r.bool() {
        Colour::rgb(red, green, blue)
    } else {
        Colour::rgba(red, green, blue, r.u8(..))
    }
}

fn point(r: &mut Fast) -> Point {
    Point::new(r.i32(-MAX..=MAX), r.i32(-MAX..=MAX))
}

fn zkey(r: &mut Fast) -> ZKey {
    loop {
        let s: String = (0..r.usize(1..=8)).map(|_| r.alphanumeric()).collect();
        if let Some(k) = ZKey::new(&s) {
            return k;
        }
    }
}

fn boxed(r: &mut Fast) -> Boxed {
    let mut b = Boxed::new(point(r), Size::new(r.i32(-MAX..=MAX), r.i32(-MAX..=MAX)));
    let styles = [
        FillStyle::Hachure,
        FillStyle::CrossHatch,
        FillStyle::Solid,
        FillStyle::Zigzag,
    ];
    if r.bool() {
        b.fill = Fill::Colour {
            colour: colour(r),
            style: styles[r.usize(..4)],
        };
    }
    b.label = r.bool().then(|| string(r));
    b
}

fn path(r: &mut Fast) -> Path {
    let t = 10 * MAX;
    let points = (0..r.usize(0..20))
        .map(|_| Point::new(r.i32(-t..=t), r.i32(-t..=t)))
        .collect();
    Path::new(point(r), points)
}

fn end(r: &mut Fast, ids: &[ElementId]) -> ArrowEnd {
    if !ids.is_empty() && r.bool() {
        let target = ids[r.usize(..ids.len())].clone();
        ArrowEnd::Bound {
            target,
            anchor: Anchor::new(r.u16(..=10_000), r.u16(..=10_000)),
        }
    } else {
        ArrowEnd::Free(point(r))
    }
}

fn kind(r: &mut Fast, ids: &[ElementId]) -> Kind {
    let heads = [
        Head::None,
        Head::Arrow,
        Head::Triangle,
        Head::Bar,
        Head::Dot,
    ];
    match r.u8(..7) {
        0 => Kind::Rect(boxed(r)),
        1 => Kind::Ellipse(boxed(r)),
        2 => Kind::Diamond(boxed(r)),
        3 => Kind::Line(path(r)),
        4 => {
            let mut a = Arrow::new(end(r, ids), end(r, ids));
            (a.heads.start, a.heads.end) = (heads[r.usize(..5)], heads[r.usize(..5)]);
            a.label = r.bool().then(|| string(r));
            Kind::Arrow(a)
        }
        5 => {
            let mut t = Text::new(point(r), string(r));
            t.font.family = [FontFamily::Hand, FontFamily::Normal, FontFamily::Code][r.usize(..3)];
            t.font.size = r.u16(..=1000);
            t.font.align = [Align::Left, Align::Center, Align::Right][r.usize(..3)];
            Kind::Text(t)
        }
        _ => Kind::Freedraw(path(r)),
    }
}

pub fn board(r: &mut Fast) -> Board {
    let mut ids_rng = Rng::seeded(r.u64(..));
    let (mut b, mut ids) = (Board::new(), Vec::new());
    for _ in 0..r.usize(0..40) {
        let id = ids_rng.element_id(&b);
        let mut e = Element::new(id.clone(), ids_rng.seed(), zkey(r), kind(r, &ids));
        e.stroke.colour = colour(r);
        (e.stroke.width, e.stroke.roughness) = (r.u16(..=1000), r.u16(..=100));
        e.stroke.dash = [Dash::Solid, Dash::Dashed, Dash::Dotted][r.usize(..3)];
        b.insert(e).unwrap();
        ids.push(id);
    }
    b
}
