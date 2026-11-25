//! The canonical writer: one byte-exact text for every board, so two clients saving the same
//! model write the same file and a merge sees only real edits.

use super::escape::quote_into;
use super::model::Unknown;
use super::{
    Align, ArrowEnd, Board, Colour, Dash, Element, Fill, FillStyle, FontFamily, Head, Kind, LIMITS,
    Point, Stroke, VERSION, usage,
};
use crate::kdl::{Entry, Node, Value};

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

/// The board as canonical KDL: the header, then every element in id order, each after a blank
/// line. Keyed unknown nodes interleave with the elements by key, an element first on a tie, and
/// unkeyed ones follow in source order.
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
    for text in &board.header {
        unknown_entry(text, &mut out);
    }
    out.push('\n');
    let mut keyed = board.keyed.iter().peekable();
    for e in board.elements() {
        while let Some((_, texts)) = keyed.next_if(|(key, _)| key.as_str() < e.id.as_str()) {
            texts.iter().for_each(|t| unknown_node(t, &mut out));
        }
        out.push('\n');
        element(e, &mut out);
    }
    for text in keyed.flat_map(|(_, texts)| texts).chain(&board.unkeyed) {
        unknown_node(text, &mut out);
    }
    check_size(out, LIMITS.hard_bytes)
}

/// The bytes [`write`] emits for unknown data held by the board itself, outside any element.
pub(crate) fn board_unknown_written(
    header: &[String],
    keyed: &std::collections::BTreeMap<String, Vec<String>>,
    unkeyed: &[String],
) -> usize {
    let mut out = String::new();
    header.iter().for_each(|t| unknown_entry(t, &mut out));
    keyed
        .values()
        .flatten()
        .chain(unkeyed)
        .for_each(|t| unknown_node(t, &mut out));
    out.len()
}

/// The bytes [`write`] emits for the element's unknown data, counted by writing it.
pub(crate) fn unknown_written(e: &Element) -> usize {
    if e.unknown.is_empty() {
        return 0;
    }
    element(e, &mut String::new())
}

/// An unknown entry on a known line or the header, after a single space.
fn unknown_entry(text: &str, out: &mut String) {
    out.push(' ');
    out.push_str(text);
}

/// An unknown top-level node after its blank line.
fn unknown_node(text: &str, out: &mut String) {
    out.push('\n');
    out.push_str(text);
    out.push('\n');
}

/// The node in unknown normal form: single spaces between entries, children four spaces deeper
/// per block, no trailing newline.
pub(crate) fn node_text(node: &Node) -> String {
    let mut out = String::new();
    node_into(node, "", &mut out);
    out
}

fn node_into(node: &Node, indent: &str, out: &mut String) {
    out.push_str(indent);
    if let Some(ty) = &node.ty {
        annotation_into(ty, out);
    }
    name_into(&node.name, out);
    for entry in &node.entries {
        out.push(' ');
        entry_into(entry, out);
    }
    if let Some(children) = &node.children {
        let inner = format!("{indent}    ");
        out.push_str(" {\n");
        for child in children {
            node_into(child, &inner, out);
            out.push('\n');
        }
        out.push_str(indent);
        out.push('}');
    }
}

/// The entry in unknown normal form: strings quoted, numbers as their source text.
pub(crate) fn entry_text(entry: &Entry) -> String {
    let mut out = String::new();
    entry_into(entry, &mut out);
    out
}

fn entry_into(entry: &Entry, out: &mut String) {
    if let Some(key) = &entry.key {
        name_into(key, out);
        out.push('=');
    }
    if let Some(ty) = &entry.ty {
        annotation_into(ty, out);
    }
    match &entry.value {
        Value::String(s) => quote_into(s, out),
        Value::Number(n) => out.push_str(n),
        Value::Bool(true) => out.push_str("#true"),
        Value::Bool(false) => out.push_str("#false"),
        Value::Null => out.push_str("#null"),
    }
}

fn annotation_into(ty: &str, out: &mut String) {
    out.push('(');
    name_into(ty, out);
    out.push(')');
}

/// A name, key or annotation: bare when it is a plain identifier that cannot read as a
/// keyword, quoted otherwise.
fn name_into(name: &str, out: &mut String) {
    let mut bytes = name.bytes();
    let plain = bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && !matches!(name, "true" | "false" | "null" | "inf" | "nan");
    if plain {
        out.push_str(name);
    } else {
        quote_into(name, out);
    }
}

/// Recounts the board against [`LIMITS`] from its elements rather than trusting the running
/// totals, so a board assembled past the mutators is still caught before it is printed.
fn over_limit(board: &Board) -> Result<(), WriteError> {
    let fail = |limit| Err(WriteError::OverLimit { limit });
    if board.len() > LIMITS.elements {
        return fail("elements");
    }
    let (mut points, mut strings) = (0, 0);
    let mut unknown = board_unknown_written(&board.header, &board.keyed, &board.unkeyed);
    for e in board.elements() {
        unknown += unknown_written(e);
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
    if unknown > LIMITS.unknown_bytes_total {
        return fail("unknown_bytes_total");
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

/// One element's block, its lines in the fixed per-kind order with `seed` second, unknown lines
/// next to last and `z` last. Returns the bytes written for unknown data.
fn element(e: &Element, out: &mut String) -> usize {
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
    let mut line = Line {
        out,
        unknown: &e.unknown,
        name: "",
        written: 0,
    };
    for text in &e.unknown.opener {
        line.unknown_entry(text);
    }
    line.out.push_str(" {\n");
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
    for text in &e.unknown.lines {
        line.unknown_line(text);
    }
    line.start("z").string(e.z.as_str()).end();
    line.out.push_str("}\n");
    line.written
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

/// Prints the child lines of one element block, each entry preceded by a single space, closing
/// each known line with its unknown entries and counting every unknown byte it writes.
struct Line<'a> {
    out: &'a mut String,
    unknown: &'a Unknown,
    /// The open line's name.
    name: &'static str,
    /// Unknown bytes written so far.
    written: usize,
}

impl Line<'_> {
    /// Opens a child line with its indentation and name.
    fn start(&mut self, name: &'static str) -> &mut Self {
        self.out.push_str("    ");
        self.out.push_str(name);
        self.name = name;
        self
    }

    fn end(&mut self) {
        let unknown = self.unknown;
        for text in unknown.entries.get(self.name).into_iter().flatten() {
            self.unknown_entry(text);
        }
        self.out.push('\n');
    }

    fn unknown_entry(&mut self, text: &str) {
        let before = self.out.len();
        unknown_entry(text, self.out);
        self.written += self.out.len() - before;
    }

    /// An unknown line, each of its own lines four spaces deeper than the element.
    fn unknown_line(&mut self, text: &str) {
        let before = self.out.len();
        for part in text.split('\n') {
            self.out.push_str("    ");
            self.out.push_str(part);
            self.out.push('\n');
        }
        self.written += self.out.len() - before;
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

    fn point(&mut self, name: &'static str, p: Point) {
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

    fn arrow_end(&mut self, name: &'static str, end: &ArrowEnd) {
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
    use super::{WriteError, check_size, entry_text, fixed, node_text, write};
    use crate::board::{Board, Boxed, Element, ElementId, Kind, LIMITS, Point, Seed, Size, ZKey};
    use crate::kdl::{ReadLimits, parse};

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

    #[test]
    fn unknown_nodes_take_the_normal_form() {
        let text = r#"(t)n  0x1F  1_000 #true #null k="a\nb" "two words"=(u)2 "inf"=x {
  child 1; other
  deep { x }
}"#;
        let nodes = parse(text, &ReadLimits::default()).unwrap();
        let want = r#"(t)n 0x1F 1_000 #true #null k="a\nb" "two words"=(u)2 "inf"="x" {
    child 1
    other
    deep {
        x
    }
}"#;
        assert_eq!(node_text(&nodes[0]), want);
        assert_eq!(entry_text(&nodes[0].entries[4]), r#"k="a\nb""#);
    }

    #[test]
    fn type_annotations_are_bare_or_quoted_like_names() {
        let nodes = parse(
            r#"("a b")n ("x")1 k=("t t")"v" ("inf")2"#,
            &ReadLimits::default(),
        )
        .unwrap();
        let want = r#"("a b")n (x)1 k=("t t")"v" ("inf")2"#;
        assert_eq!(node_text(&nodes[0]), want);
        // The normal form must itself parse, to the same text.
        assert_eq!(
            node_text(&parse(want, &ReadLimits::default()).unwrap()[0]),
            want
        );
    }
}
