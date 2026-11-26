//! The element model: ids, seeds, geometry, styles and the seven element kinds.

use std::collections::BTreeMap;
use std::fmt;

use super::ZKey;

/// Longest element id, in characters.
const MAX_ID_LEN: usize = 32;

/// An element's id: 1 to 32 characters of `[0-9A-Za-z_-]`, unique within a board.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ElementId(String);

impl ElementId {
    /// The id spelled `s`, or `None` when `s` is empty, too long or holds another character.
    pub fn new(s: &str) -> Option<Self> {
        let valid = (1..=MAX_ID_LEN).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        valid.then(|| Self(s.to_owned()))
    }

    /// The id's text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The seed of an element's hand-drawn jitter, 0 to [`Seed::MAX`] so it fits every
/// renderer's signed 32-bit integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct Seed(u32);

impl Seed {
    /// The largest seed, 2^31 - 1.
    pub const MAX: u32 = (1 << 31) - 1;

    /// The seed `n`, or `None` above [`Seed::MAX`].
    pub const fn new(n: u32) -> Option<Self> {
        if n <= Self::MAX { Some(Self(n)) } else { None }
    }

    /// The seed `n`, or [`Seed::MAX`] when `n` is above it.
    pub(crate) const fn clamped(n: u32) -> Self {
        if n <= Self::MAX {
            Self(n)
        } else {
            Self(Self::MAX)
        }
    }

    /// The seed's value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A position on the board, in whole board units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Point {
    /// Rightward offset.
    pub x: i32,
    /// Downward offset.
    pub y: i32,
}

impl Point {
    /// The point `(x, y)`.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A box's extent, in whole board units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Size {
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

impl Size {
    /// The size `w` by `h`.
    pub const fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }
}

/// Where an arrow binds on its target's box, as fractions of its size in units of 1e-4:
/// `0..=10000` on each axis, `(5000, 5000)` being the centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Anchor {
    /// Fraction of the width from the left edge.
    pub x: u16,
    /// Fraction of the height from the top edge.
    pub y: u16,
}

impl Anchor {
    /// The anchor `(x, y)`.
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// An sRGB colour, opaque or with an explicit alpha. The alpha is kept as written, so
/// `#rrggbbff` stays eight digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Colour {
    rgb: [u8; 3],
    alpha: Option<u8>,
}

impl Colour {
    /// The colour `#rrggbb`.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            rgb: [r, g, b],
            alpha: None,
        }
    }

    /// The colour `#rrggbbaa`.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            rgb: [r, g, b],
            alpha: Some(a),
        }
    }

    /// The colour `#rrggbb` or `#rrggbbaa` in hex of any case, or `None` for any other text.
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        // The text is ASCII hex from here on, so every two-byte slice is a whole pair.
        let byte = |i: usize| {
            hex.get(i..i + 2)
                .and_then(|p| u8::from_str_radix(p, 16).ok())
        };
        let rgb = [byte(0)?, byte(2)?, byte(4)?];
        let alpha = if hex.len() == 8 { Some(byte(6)?) } else { None };
        Some(Self { rgb, alpha })
    }

    /// `[r, g, b, a]`, alpha 255 when the colour has none.
    pub const fn channels(self) -> [u8; 4] {
        let [r, g, b] = self.rgb;
        let a = match self.alpha {
            Some(a) => a,
            None => 255,
        };
        [r, g, b, a]
    }
}

impl fmt::Display for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.rgb;
        write!(f, "#{r:02x}{g:02x}{b:02x}")?;
        match self.alpha {
            Some(a) => write!(f, "{a:02x}"),
            None => Ok(()),
        }
    }
}

/// A stroke's dash pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Dash {
    /// A continuous line.
    Solid,
    /// Long dashes.
    Dashed,
    /// Dots.
    Dotted,
}

/// How an element's outline is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Stroke {
    /// Line colour.
    pub colour: Colour,
    /// Line width in tenths, `0..=1000`.
    pub width: u16,
    /// Hand-drawn wobble in tenths, `0..=100`.
    pub roughness: u16,
    /// Dash pattern.
    pub dash: Dash,
}

impl Default for Stroke {
    /// `#1e1e1e`, width 2, roughness 1, solid.
    fn default() -> Self {
        Self {
            colour: Colour::rgb(0x1e, 0x1e, 0x1e),
            width: 20,
            roughness: 10,
            dash: Dash::Solid,
        }
    }
}

/// The pattern a fill is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FillStyle {
    /// Parallel diagonal strokes.
    Hachure,
    /// Crossed diagonal strokes.
    CrossHatch,
    /// A flat fill.
    Solid,
    /// A zigzag stroke.
    Zigzag,
}

/// A box's interior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Fill {
    /// Unfilled.
    #[default]
    None,
    /// Filled with `colour` in `style`.
    Colour {
        /// Fill colour.
        colour: Colour,
        /// Fill pattern.
        style: FillStyle,
    },
}

/// A text's typeface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontFamily {
    /// The hand-written face.
    Hand,
    /// The plain sans-serif face.
    Normal,
    /// The monospace face.
    Code,
}

/// A text's horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Align {
    /// Flush left.
    Left,
    /// Centred.
    Center,
    /// Flush right.
    Right,
}

/// How a text is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Font {
    /// Typeface.
    pub family: FontFamily,
    /// Size in tenths, `0..=1000`.
    pub size: u16,
    /// Alignment.
    pub align: Align,
}

impl Default for Font {
    /// Hand, size 20, left.
    fn default() -> Self {
        Self {
            family: FontFamily::Hand,
            size: 200,
            align: Align::Left,
        }
    }
}

/// The mark at one end of a line or arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Head {
    /// No mark.
    None,
    /// An open arrowhead.
    Arrow,
    /// A closed triangle.
    Triangle,
    /// A perpendicular bar.
    Bar,
    /// A dot.
    Dot,
}

/// The marks at both ends of an arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Heads {
    /// Mark at the `from` end.
    pub start: Head,
    /// Mark at the `to` end.
    pub end: Head,
}

impl Default for Heads {
    /// No start mark, an arrowhead at the end.
    fn default() -> Self {
        Self {
            start: Head::None,
            end: Head::Arrow,
        }
    }
}

/// One end of an arrow: bound to another element or free on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArrowEnd {
    /// Attached to `target` at `anchor` on its box.
    Bound {
        /// The element the end follows.
        target: ElementId,
        /// Where on the target's box the end sits.
        anchor: Anchor,
    },
    /// Loose at a board point.
    Free(Point),
}

/// The body of a rectangle, ellipse or diamond.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Boxed {
    /// Top-left corner.
    pub at: Point,
    /// Extent.
    pub size: Size,
    /// Interior.
    pub fill: Fill,
    /// Text drawn inside, if any.
    pub label: Option<String>,
}

impl Boxed {
    /// An unfilled, unlabelled box.
    pub fn new(at: Point, size: Size) -> Self {
        Self {
            at,
            size,
            fill: Fill::None,
            label: None,
        }
    }
}

/// The body of a line or freehand stroke.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Path {
    /// Origin the points are relative to.
    pub at: Point,
    /// Vertices in tenths of a unit, relative to `at`.
    pub points: Vec<Point>,
}

impl Path {
    /// The path through `points`, relative to `at`.
    pub fn new(at: Point, points: Vec<Point>) -> Self {
        Self { at, points }
    }
}

/// The body of an arrow.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Arrow {
    /// Start end.
    pub from: ArrowEnd,
    /// Finish end.
    pub to: ArrowEnd,
    /// End marks.
    pub heads: Heads,
    /// Text drawn along the arrow, if any.
    pub label: Option<String>,
}

impl Arrow {
    /// An unlabelled arrow with the default heads.
    pub fn new(from: ArrowEnd, to: ArrowEnd) -> Self {
        Self {
            from,
            to,
            heads: Heads::default(),
            label: None,
        }
    }
}

/// The body of a free text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Text {
    /// Top-left corner.
    pub at: Point,
    /// The text itself.
    pub content: String,
    /// How it is set.
    pub font: Font,
}

impl Text {
    /// `content` at `at` in the default font.
    pub fn new(at: Point, content: String) -> Self {
        Self {
            at,
            content,
            font: Font::default(),
        }
    }
}

/// What an element is, with the fields only that kind has.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// A rectangle.
    Rect(Boxed),
    /// An ellipse inscribed in its box.
    Ellipse(Boxed),
    /// A diamond inscribed in its box.
    Diamond(Boxed),
    /// A straight polyline.
    Line(Path),
    /// An arrow between two ends.
    Arrow(Arrow),
    /// A free text.
    Text(Text),
    /// A freehand stroke.
    Freedraw(Path),
}

/// One drawn thing on a board.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Element {
    /// Unique id within the board.
    pub id: ElementId,
    /// Jitter seed.
    pub seed: Seed,
    /// Outline.
    pub stroke: Stroke,
    /// Paint-order key.
    pub z: ZKey,
    /// Kind and its fields.
    pub kind: Kind,
    /// Data this build does not understand, kept so a save writes it back.
    pub(crate) unknown: Unknown,
}

/// An element's unknown data in the normal form `write::node_text` and `write::entry_text` give,
/// so loading and writing it again is byte-stable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Unknown {
    /// Entries after the id on the element's block opener, in source order.
    pub(crate) opener: Vec<String>,
    /// Entries after a known line's own entries, by line name, in source order.
    pub(crate) entries: BTreeMap<String, Vec<String>>,
    /// Whole lines this build does not know, in source order.
    pub(crate) lines: Vec<String>,
}

impl Unknown {
    pub(crate) fn is_empty(&self) -> bool {
        self.opener.is_empty() && self.entries.is_empty() && self.lines.is_empty()
    }
}

impl Element {
    /// An element with the default stroke.
    pub fn new(id: ElementId, seed: Seed, z: ZKey, kind: Kind) -> Self {
        Self {
            id,
            seed,
            stroke: Stroke::default(),
            z,
            kind,
            unknown: Unknown::default(),
        }
    }
}
