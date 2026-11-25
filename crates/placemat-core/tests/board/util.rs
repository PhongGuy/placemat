//! Builders shared by the board tests.

use placemat_core::board::{
    Align, Anchor, Arrow, ArrowEnd, Board, Boxed, Colour, Dash, Element, ElementId, Fill,
    FillStyle, FontFamily, Head, Kind, LIMITS, LimitReached, Path, Point, Seed, Size, Text, ZKey,
    load,
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

/// Every kind in canonical form; `all_kinds_board` is its model.
pub const ALL_KINDS: &str = r##"placemat-board version=0

rect "3kTq8wZp" {
    at 80 120
    seed 1640275519
    size 200 100
    fill "#a5d8ff" style=hachure
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    label "Web client"
    z "a1"
}

rect "7hWc4nRd" {
    at 480 120
    seed 288140027
    size 200 100
    fill "#b2f2bb" style=cross-hatch
    stroke "#1e1e1e" width=2.5 roughness=0 dash=dashed
    z "a2"
}

arrow "9rLm2xQe" {
    from "3kTq8wZp" x=1 y=0.5
    seed 905113748
    to "7hWc4nRd" x=0 y=0.5
    head start=none end=arrow
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    label "HTTPS"
    z "a3"
}

arrow "aFree001" {
    from 120 340
    seed 7
    to -40 -7
    head start=bar end=triangle
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    z "a4"
}

ellipse "b2Dy6sKu" {
    at 500 320
    seed 1197760311
    size 160 90
    fill "#ffec99" style=zigzag
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    label "Postgres"
    z "a6"
}

diamond "d1Mn5pQr" {
    at -10 0
    seed 0
    size 0 0
    fill "#ffec99ff" style=solid
    stroke "#868e96" width=0.5 roughness=10 dash=dotted
    label ""
    z "a5"
}

line "e5Gp0vXa" {
    at 580 220
    seed 2034019962
    points 0 0 0 100 -0.5 1.5
    stroke "#868e96" width=1 roughness=1 dash=dashed
    z "a7"
}

freedraw "f7Hj2kLm" {
    at 0 0
    seed 2147483647
    points 0.1 0.2 16777216 -16777216
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    z "a8"
}

text "q1Nf3jTb" {
    at 80 260
    seed 51807420
    content "Sketch of the v1 request path.\nNot to scale."
    font family=normal size=20.5 align=center
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    z "a9"
}

text "q2Code00" {
    at -1 -1
    seed 1
    content "fn main() {}"
    font family=code size=12 align=right
    stroke "#1e1e1e" width=2 roughness=1 dash=solid
    z "aA"
}
"##;

pub fn all_kinds_board() -> Board {
    let grey = Colour::rgb(0x86, 0x8e, 0x96);
    let mut web = Boxed::new(Point::new(80, 120), Size::new(200, 100));
    web.fill = Fill::Colour {
        colour: Colour::rgb(0xa5, 0xd8, 0xff),
        style: FillStyle::Hachure,
    };
    web.label = Some("Web client".to_owned());
    let mut api = Boxed::new(Point::new(480, 120), Size::new(200, 100));
    api.fill = Fill::Colour {
        colour: Colour::rgb(0xb2, 0xf2, 0xbb),
        style: FillStyle::CrossHatch,
    };
    let mut api = el("7hWc4nRd", 288_140_027, "a2", Kind::Rect(api));
    (api.stroke.width, api.stroke.roughness, api.stroke.dash) = (25, 0, Dash::Dashed);
    let mut http = Arrow::new(
        ArrowEnd::Bound {
            target: id("3kTq8wZp"),
            anchor: Anchor::new(10_000, 5_000),
        },
        ArrowEnd::Bound {
            target: id("7hWc4nRd"),
            anchor: Anchor::new(0, 5_000),
        },
    );
    http.label = Some("HTTPS".to_owned());
    let mut free = Arrow::new(
        ArrowEnd::Free(Point::new(120, 340)),
        ArrowEnd::Free(Point::new(-40, -7)),
    );
    (free.heads.start, free.heads.end) = (Head::Bar, Head::Triangle);
    let mut db = Boxed::new(Point::new(500, 320), Size::new(160, 90));
    db.fill = Fill::Colour {
        colour: Colour::rgb(0xff, 0xec, 0x99),
        style: FillStyle::Zigzag,
    };
    db.label = Some("Postgres".to_owned());
    let mut gem = Boxed::new(Point::new(-10, 0), Size::new(0, 0));
    gem.fill = Fill::Colour {
        colour: Colour::rgba(0xff, 0xec, 0x99, 0xff),
        style: FillStyle::Solid,
    };
    gem.label = Some(String::new());
    let mut gem = el("d1Mn5pQr", 0, "a5", Kind::Diamond(gem));
    gem.stroke.colour = grey;
    (gem.stroke.width, gem.stroke.roughness, gem.stroke.dash) = (5, 100, Dash::Dotted);
    let line = Path::new(
        Point::new(580, 220),
        vec![Point::new(0, 0), Point::new(0, 1000), Point::new(-5, 15)],
    );
    let mut line = el("e5Gp0vXa", 2_034_019_962, "a7", Kind::Line(line));
    line.stroke.colour = grey;
    (line.stroke.width, line.stroke.dash) = (10, Dash::Dashed);
    let pen = Path::new(
        Point::new(0, 0),
        vec![Point::new(1, 2), Point::new(167_772_160, -167_772_160)],
    );
    let mut note = Text::new(
        Point::new(80, 260),
        "Sketch of the v1 request path.\nNot to scale.".to_owned(),
    );
    (note.font.family, note.font.size, note.font.align) = (FontFamily::Normal, 205, Align::Center);
    let mut code = Text::new(Point::new(-1, -1), "fn main() {}".to_owned());
    (code.font.family, code.font.size, code.font.align) = (FontFamily::Code, 120, Align::Right);
    let mut b = Board::new();
    for e in [
        el("3kTq8wZp", 1_640_275_519, "a1", Kind::Rect(web)),
        api,
        el("9rLm2xQe", 905_113_748, "a3", Kind::Arrow(http)),
        el("aFree001", 7, "a4", Kind::Arrow(free)),
        el("b2Dy6sKu", 1_197_760_311, "a6", Kind::Ellipse(db)),
        gem,
        line,
        el("f7Hj2kLm", Seed::MAX, "a8", Kind::Freedraw(pen)),
        el("q1Nf3jTb", 51_807_420, "a9", Kind::Text(note)),
        el("q2Code00", 1, "aA", Kind::Text(code)),
    ] {
        b.insert(e).unwrap();
    }
    b
}

/// A fixture under `tests/fixtures/boards/`, read as text.
pub fn fixture(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/boards")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn widest(e: &mut Element) {
    e.stroke.colour = Colour::rgba(0xff, 0xff, 0xff, 0xff);
    (e.stroke.width, e.stroke.roughness, e.stroke.dash) = (999, 99, Dash::Dashed);
}

/// A board filled to every limit: the unknown-bytes, point, element and string budgets.
pub fn limits_board() -> Board {
    let blob = "a".repeat(LIMITS.unknown_bytes_total - 64);
    let start = format!("placemat-board version=0\n\nwidget \"w\" blob=\"{blob}\"\n");
    let mut b = load(start.as_bytes()).unwrap().board;
    let wid = |i: usize| id(&format!("{i:0>32}"));
    let zmax = z(&"z".repeat(64));
    let t = 167_772_159;
    let pens = LIMITS.points_total / LIMITS.points_per_element;
    for i in 0..pens {
        let path = Path::new(
            Point::new(-(1 << 24), -(1 << 24)),
            vec![Point::new(-t, -t); LIMITS.points_per_element],
        );
        let mut e = Element::new(wid(i), seed(Seed::MAX), zmax.clone(), Kind::Freedraw(path));
        widest(&mut e);
        b.insert(e).unwrap();
    }
    let per_label = LIMITS.string_bytes_total / (LIMITS.elements - pens);
    for i in pens..LIMITS.elements {
        let end = |n: usize| ArrowEnd::Bound {
            target: wid(n),
            anchor: Anchor::new(1234, 5678),
        };
        let mut a = Arrow::new(end(i % pens), end((i + 1) % pens));
        (a.heads.start, a.heads.end) = (Head::Triangle, Head::Triangle);
        a.label = Some("a".repeat(per_label));
        let mut e = Element::new(wid(i), seed(Seed::MAX), zmax.clone(), Kind::Arrow(a));
        widest(&mut e);
        b.insert(e).unwrap();
    }
    b
}
