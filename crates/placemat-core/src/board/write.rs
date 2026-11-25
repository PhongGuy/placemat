//! The canonical writer: one byte-exact text for every board, so two clients saving the same
//! model write the same file and a merge sees only real edits.

use super::escape::quote_into;
use super::{
    Align, ArrowEnd, Board, Colour, Dash, Element, Fill, FillStyle, FontFamily, Head, Kind, LIMITS,
    Point, Stroke, VERSION, usage,
};

/// Why a board was not written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
    /// The board was read at a newer format version than this build writes.
    #[error("board version {found} is newer than this Placemat, so it is read-only")]
    ReadOnly {
        /// The version the board was read at.
        found: u32,
    },
    /// The written text would pass [`LIMITS`]`.hard_bytes`.
    #[error("board is {bytes} bytes, over the 16 MiB limit")]
    TooLarge {
        /// The size the text would have had.
        bytes: usize,
    },
    /// The board holds more than [`LIMITS`] allows, which the board API never builds; nothing is written.
    #[error("board {limit} limit exceeded")]
    OverLimit {
        /// The exceeded [`super::Limits`] field.
        limit: &'static str,
    },
}

/// Tenths (widths, sizes, path points) are written with one fraction digit at most.
const TENTHS: u32 = 1;
/// Anchors are fractions in units of 1e-4.
const ANCHOR: u32 = 4;

/// The board as canonical KDL: the header, then every element in id order, each after a blank line.
pub fn write(board: &Board) -> Result<String, WriteError> {
    if board.version() > VERSION {
        return Err(WriteError::ReadOnly {
            found: board.version(),
        });
    }
    over_limit(board)?;
    let mut out = String::new();
    out.push_str("placemat-board version=");
    out.push_str(&VERSION.to_string());
    out.push('\n');
    for e in board.elements() {
        out.push('\n');
        element(e, &mut out);
    }
    check_size(out, LIMITS.hard_bytes)
}

/// Recounts the board against [`LIMITS`] from its elements rather than trusting the running
/// totals, so a board assembled past the mutators is still caught before it is printed.
fn over_limit(board: &Board) -> Result<(), WriteError> {
    let fail = |limit| Err(WriteError::OverLimit { limit });
    if board.len() > LIMITS.elements {
        return fail("elements");
    }
    let (mut points, mut strings) = (0, 0);
    for e in board.elements() {
        let u = usage(e);
        if u.points > LIMITS.points_per_element {
            return fail("points_per_element");
        }
        if u.string_bytes > LIMITS.string_bytes_each {
            return fail("string_bytes_each");
        }
        points += u.points;
        strings += u.string_bytes;
    }
    if points > LIMITS.points_total {
        return fail("points_total");
    }
    if strings > LIMITS.string_bytes_total {
        return fail("string_bytes_total");
    }
    Ok(())
}

/// The text itself, or `TooLarge` when it is longer than `hard` bytes.
fn check_size(text: String, hard: usize) -> Result<String, WriteError> {
    if text.len() > hard {
        return Err(WriteError::TooLarge { bytes: text.len() });
    }
    Ok(text)
}

/// One element's block, its lines in the fixed per-kind order with `seed` second and `z` last.
fn element(e: &Element, out: &mut String) {
    let kind = match &e.kind {
        Kind::Rect(_) => "rect",
        Kind::Ellipse(_) => "ellipse",
        Kind::Diamond(_) => "diamond",
        Kind::Line(_) => "line",
        Kind::Arrow(_) => "arrow",
        Kind::Text(_) => "text",
        Kind::Freedraw(_) => "freedraw",
    };
    out.push_str(kind);
    out.push(' ');
    quote_into(e.id.as_str(), out);
    out.push_str(" {\n");
    let mut line = Line { out };
    match &e.kind {
        Kind::Rect(b) | Kind::Ellipse(b) | Kind::Diamond(b) => {
            line.point("at", b.at);
            line.seed(e);
            line.start("size").int(b.size.w).int(b.size.h).end();
            match b.fill {
                Fill::None => line.start("fill").bare("none").end(),
                Fill::Colour { colour, style } => {
                    let style = match style {
                        FillStyle::Hachure => "hachure",
                        FillStyle::CrossHatch => "cross-hatch",
                        FillStyle::Solid => "solid",
                        FillStyle::Zigzag => "zigzag",
                    };
                    line.start("fill").colour(colour).prop("style", style).end();
                }
            }
            line.stroke(&e.stroke);
            line.label(b.label.as_deref());
        }
        Kind::Line(p) | Kind::Freedraw(p) => {
            line.point("at", p.at);
            line.seed(e);
            let points = line.start("points");
            for q in &p.points {
                points.fixed(q.x.into(), TENTHS).fixed(q.y.into(), TENTHS);
            }
            points.end();
            line.stroke(&e.stroke);
        }
        Kind::Arrow(a) => {
            line.arrow_end("from", &a.from);
            line.seed(e);
            line.arrow_end("to", &a.to);
            line.start("head")
                .prop("start", head(a.heads.start))
                .prop("end", head(a.heads.end))
                .end();
            line.stroke(&e.stroke);
            line.label(a.label.as_deref());
        }
        Kind::Text(t) => {
            line.point("at", t.at);
            line.seed(e);
            line.start("content").string(&t.content).end();
            let family = match t.font.family {
                FontFamily::Hand => "hand",
                FontFamily::Normal => "normal",
                FontFamily::Code => "code",
            };
            let align = match t.font.align {
                Align::Left => "left",
                Align::Center => "center",
                Align::Right => "right",
            };
            line.start("font")
                .prop("family", family)
                .prop("size", &fixed(t.font.size.into(), TENTHS))
                .prop("align", align)
                .end();
            line.stroke(&e.stroke);
        }
    }
    line.start("z").string(e.z.as_str()).end();
    line.out.push_str("}\n");
}

fn head(h: Head) -> &'static str {
    match h {
        Head::None => "none",
        Head::Arrow => "arrow",
        Head::Triangle => "triangle",
        Head::Bar => "bar",
        Head::Dot => "dot",
    }
}

/// Prints the child lines of one element block, each entry preceded by a single space.
struct Line<'a> {
    out: &'a mut String,
}

impl Line<'_> {
    /// Opens a child line with its indentation and name.
    fn start(&mut self, name: &str) -> &mut Self {
        self.out.push_str("    ");
        self.out.push_str(name);
        self
    }

    fn end(&mut self) {
        self.out.push('\n');
    }

    fn bare(&mut self, s: &str) -> &mut Self {
        self.out.push(' ');
        self.out.push_str(s);
        self
    }

    fn int(&mut self, n: i32) -> &mut Self {
        self.bare(&n.to_string())
    }

    fn fixed(&mut self, value: i64, digits: u32) -> &mut Self {
        self.bare(&fixed(value, digits))
    }

    fn string(&mut self, s: &str) -> &mut Self {
        self.out.push(' ');
        quote_into(s, self.out);
        self
    }

    fn colour(&mut self, c: Colour) -> &mut Self {
        self.string(&c.to_string())
    }

    fn prop(&mut self, key: &str, value: &str) -> &mut Self {
        self.out.push(' ');
        self.out.push_str(key);
        self.out.push('=');
        self.out.push_str(value);
        self
    }

    fn point(&mut self, name: &str, p: Point) {
        self.start(name).int(p.x).int(p.y).end();
    }

    fn seed(&mut self, e: &Element) {
        self.start("seed").bare(&e.seed.get().to_string()).end();
    }

    fn stroke(&mut self, s: &Stroke) {
        let dash = match s.dash {
            Dash::Solid => "solid",
            Dash::Dashed => "dashed",
            Dash::Dotted => "dotted",
        };
        self.start("stroke")
            .colour(s.colour)
            .prop("width", &fixed(s.width.into(), TENTHS))
            .prop("roughness", &fixed(s.roughness.into(), TENTHS))
            .prop("dash", dash)
            .end();
    }

    /// The label line, written only when a label is present, even an empty one.
    fn label(&mut self, label: Option<&str>) {
        if let Some(label) = label {
            self.start("label").string(label).end();
        }
    }

    fn arrow_end(&mut self, name: &str, end: &ArrowEnd) {
        match end {
            ArrowEnd::Bound { target, anchor } => {
                self.start(name)
                    .string(target.as_str())
                    .prop("x", &fixed(anchor.x.into(), ANCHOR))
                    .prop("y", &fixed(anchor.y.into(), ANCHOR))
                    .end();
            }
            ArrowEnd::Free(p) => self.point(name, *p),
        }
    }
}

/// `value` scaled down by `10^digits` as the shortest exact decimal: `25` at one digit is
/// `2.5`, `20` is `2`, and `-5` keeps its sign as `-0.5`.
pub(crate) fn fixed(value: i64, digits: u32) -> String {
    let scale = 10_u64.pow(digits);
    let magnitude = value.unsigned_abs();
    let mut text = String::new();
    if value < 0 {
        text.push('-');
    }
    text.push_str(&(magnitude / scale).to_string());
    let rem = magnitude % scale;
    if rem != 0 {
        let width = digits as usize;
        let frac = format!("{rem:0width$}");
        text.push('.');
        text.push_str(frac.trim_end_matches('0'));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{WriteError, check_size, fixed, write};
    use crate::board::{Board, Boxed, Element, ElementId, Kind, LIMITS, Point, Seed, Size, ZKey};

    #[test]
    fn fixed_point_is_the_shortest_exact_decimal() {
        for (value, digits, want) in [
            (0, 1, "0"),
            (20, 1, "2"),
            (25, 1, "2.5"),
            (-5, 1, "-0.5"),
            (10_000, 4, "1"),
            (500, 4, "0.05"),
            (1234, 4, "0.1234"),
            (-167_772_159, 1, "-16777215.9"),
        ] {
            assert_eq!(fixed(value, digits), want, "{value} at {digits} digits");
        }
    }

    #[test]
    fn output_over_the_hard_cap_is_refused() {
        assert_eq!(check_size("abc".to_owned(), 3), Ok("abc".to_owned()));
        assert_eq!(
            check_size("abcd".to_owned(), 3),
            Err(WriteError::TooLarge { bytes: 4 })
        );
    }

    #[test]
    fn the_limit_backstop_refuses_a_board_built_past_the_api() {
        let mut b = Board::new();
        for i in 0..=LIMITS.elements {
            let id = ElementId::new(&format!("e{i}")).unwrap();
            let kind = Kind::Rect(Boxed::new(Point::new(0, 0), Size::new(1, 1)));
            let e = Element::new(
                id.clone(),
                Seed::new(1).unwrap(),
                ZKey::new("a1").unwrap(),
                kind,
            );
            b.elements.insert(id, e);
        }
        assert_eq!(write(&b), Err(WriteError::OverLimit { limit: "elements" }));
    }
}
