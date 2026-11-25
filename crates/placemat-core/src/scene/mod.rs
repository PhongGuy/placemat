//! Scene geometry for a board: the value types that carry each element's hand-drawn world
//! geometry, its label and its bounds, plus the text measurer that layout is injected with.
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr
)]

mod rough;

/// Measures text for layout, so the core stays free of any font engine.
///
/// Implementations may return any `f32`; callers sanitise non-finite or negative results.
pub trait TextMeasure: Send + Sync {
    /// The advance width of one line (no line breaks) at font size `size`, in world units.
    fn line_width(&self, line: &str, size: f32) -> f32;
    /// The distance between baselines at font size `size`, in world units.
    fn line_height(&self, size: f32) -> f32;
}

/// A monospace measurer: every character advances by the same fraction of the font size.
/// Used headless and in tests, where no real font is loaded.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedAdvance {
    /// Width of one character, as a fraction of the font size (an em fraction).
    pub advance: f32,
    /// Distance between baselines, as a fraction of the font size.
    pub line_height: f32,
}

impl FixedAdvance {
    /// A measurer with the given character advance and line height, both em fractions.
    pub const fn new(advance: f32, line_height: f32) -> Self {
        Self {
            advance,
            line_height,
        }
    }
}

impl TextMeasure for FixedAdvance {
    // Characters, not bytes: a multi-byte character is still one glyph wide.
    fn line_width(&self, line: &str, size: f32) -> f32 {
        line.chars().count() as f32 * self.advance * size
    }

    fn line_height(&self, size: f32) -> f32 {
        self.line_height * size
    }
}

/// An axis-aligned rectangle in world units, with `x0 <= x1` and `y0 <= y1` when built by
/// [`Rect::new`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x0: f64,
    /// Top edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Bottom edge.
    pub y1: f64,
}

impl Rect {
    /// The rectangle spanning two corners given in any order; each axis is sorted.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    /// The horizontal extent.
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// The vertical extent.
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// Whether the two closed rectangles share any point; touching edges count.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
    }

    /// The smallest rectangle covering both.
    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// What an [`OpSet`] paints: the stroke, a solid fill, or a hachure-style sketch fill.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKind {
    /// Outline strokes, painted with the element's stroke colour.
    Stroke,
    /// A closed region filled solid with the element's fill colour.
    Fill,
    /// Fill lines (hachure and friends), stroked with the element's fill colour.
    Sketch,
}

/// One path command, in world units local to the element's origin.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// Start a new subpath at `(x, y)`.
    Move(f32, f32),
    /// A straight segment to `(x, y)`.
    Line(f32, f32),
    /// A cubic Bezier: control points `[c1x, c1y, c2x, c2y]`, then the end `[x, y]`.
    Cubic([f32; 6]),
}

/// A run of path commands painted one way.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct OpSet {
    /// How the set is painted.
    pub kind: SetKind,
    /// The path commands, in order.
    pub ops: Vec<Op>,
}

/// Laid-out text: its lines, the font size in world units and the block's top-left corner.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// The lines in order, already broken and capped.
    pub lines: Vec<String>,
    /// Font size in world units.
    pub world_size: f32,
    /// Top-left corner of the text block, local to the element's origin.
    pub at: (f32, f32),
}

/// An element's cached geometry: the hand-drawn sets, a plain outline for selection and
/// low detail, the label, and whether the box fallback replaced the rough output.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Geometry {
    /// The hand-drawn strokes and fills, in paint order.
    pub rough: Vec<OpSet>,
    /// The smooth outline of the same shape.
    pub outline: Vec<OpSet>,
    /// The element's text, if any.
    pub label: Option<Label>,
    /// True when generation failed or produced bad values and a plain box stands in.
    pub fallback: bool,
}

#[cfg(test)]
mod tests;
