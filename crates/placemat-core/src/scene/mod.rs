//! A board turned into cached, sanitised, hand-drawn world geometry. [`Scene`] holds one entry
//! per element in paint order, answers what a view rectangle touches with a flat bounds scan,
//! and stays current only through [`Scene::invalidate`], which also redraws bound arrows. Text
//! is laid out through an injected [`TextMeasure`]; [`Camera`] and the view rules map world
//! units to screen pixels and pick the detail level, dash pattern and label size.
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::collections::{BTreeMap, BTreeSet};

use crate::board::{ArrowEnd, Board, Element, ElementId, Kind, ZKey};

mod label;
mod rough;
mod view;

pub use view::{
    Camera, DASHES_MAX, DOT_BELOW_PX, Detail, LABEL_MAX_PX, LABEL_MIN_PX, SIMPLE_BELOW_PX,
    dash_array, detail, label_px,
};

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
    /// The lines in order, already broken, capped and expanded for display.
    pub lines: Vec<LabelLine>,
    /// Font size in world units.
    pub world_size: f32,
    /// Top-left corner of the text block, local to the element's origin.
    pub at: (f32, f32),
}

/// One label line as it is shown: hidden code points tagged, tabs as spaces.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct LabelLine {
    /// The display text: tags inlined, tabs as spaces.
    pub text: String,
    /// Byte ranges of `text` that are tags, ascending and disjoint.
    pub hidden: Vec<std::ops::Range<usize>>,
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

/// One element's cached geometry with the paint key it sorts by and the ids its ends follow.
#[derive(Debug, Clone)]
struct Entry {
    id: ElementId,
    z: ZKey,
    targets: Vec<ElementId>,
    built: rough::Built,
}

/// A board's cached geometry in paint order, culled by a flat bounds scan.
///
/// Edits reach it only through [`Scene::invalidate`], which also regenerates every arrow bound
/// to the changed element.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    entries: Vec<Entry>,
    index: BTreeMap<ElementId, usize>,
    arrows: BTreeMap<ElementId, BTreeSet<ElementId>>,
}

/// An element the view rectangle touches, as [`Scene::visible`] yields it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Visible<'a> {
    /// The element's id.
    pub id: &'a ElementId,
    /// The element's position in the whole board's paint order.
    pub rank: u32,
    /// World bounds of everything the element paints.
    pub bounds: Rect,
    /// The world point the geometry's ops are local to.
    pub origin: (f64, f64),
    /// The cached geometry.
    pub geometry: &'a Geometry,
}

impl Scene {
    /// Generates every element of `board` in paint order, laying out text with `m`.
    pub fn build(board: &Board, m: &dyn TextMeasure) -> Scene {
        let mut scene = Scene {
            entries: board.paint_order().map(|e| entry(e, board, m)).collect(),
            ..Scene::default()
        };
        for e in &scene.entries {
            link(&mut scene.arrows, e);
        }
        scene.reindex();
        scene
    }

    /// Brings the cache in line with `board` after the element `id` was added, changed or
    /// removed, and regenerates every arrow bound to it.
    pub fn invalidate(&mut self, board: &Board, m: &dyn TextMeasure, id: &ElementId) {
        // Drop the stale entry and its links, then file the fresh one at its paint position.
        if let Some(old) = self
            .index
            .get(id)
            .copied()
            .filter(|&i| i < self.entries.len())
        {
            let old = self.entries.remove(old);
            unlink(&mut self.arrows, &old);
        }
        if let Some(e) = board.get(id) {
            let fresh = entry(e, board, m);
            let at = self
                .entries
                .partition_point(|x| (&x.z, &x.id) < (&fresh.z, &fresh.id));
            link(&mut self.arrows, &fresh);
            self.entries.insert(at, fresh);
        }
        self.reindex();
        // Arrows keep their paint position, so the index stays valid while they regenerate.
        let bound: Vec<ElementId> = self
            .arrows
            .get(id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        for arrow in bound {
            let (Some(e), Some(&i)) = (board.get(&arrow), self.index.get(&arrow)) else {
                continue;
            };
            if let Some(slot) = self.entries.get_mut(i) {
                slot.built = rough::build(e, board, m);
            }
        }
    }

    /// The union of every element's bounds, or `None` for an empty scene.
    pub fn bounds(&self) -> Option<Rect> {
        self.entries
            .iter()
            .map(|e| e.built.bounds)
            .reduce(|a, b| a.union(&b))
    }

    /// The elements whose bounds touch `view`, in paint order.
    pub fn visible(&self, view: Rect) -> impl Iterator<Item = Visible<'_>> {
        self.entries.iter().enumerate().filter_map(move |(i, e)| {
            if !e.built.bounds.intersects(&view) {
                return None;
            }
            Some(Visible {
                id: &e.id,
                rank: u32::try_from(i).ok()?,
                bounds: e.built.bounds,
                origin: e.built.origin,
                geometry: &e.built.geometry,
            })
        })
    }

    fn reindex(&mut self) {
        self.index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
    }
}

/// Generates one element and records the ids its arrow ends follow, if it is an arrow.
fn entry(e: &Element, board: &Board, m: &dyn TextMeasure) -> Entry {
    let targets = if let Kind::Arrow(a) = &e.kind {
        [&a.from, &a.to]
            .into_iter()
            .filter_map(|end| match end {
                ArrowEnd::Bound { target, .. } => Some(target.clone()),
                ArrowEnd::Free(_) => None,
            })
            .collect()
    } else {
        Vec::new()
    };
    Entry {
        id: e.id.clone(),
        z: e.z.clone(),
        targets,
        built: rough::build(e, board, m),
    }
}

/// Files an arrow under each element its ends follow.
fn link(arrows: &mut BTreeMap<ElementId, BTreeSet<ElementId>>, e: &Entry) {
    for t in &e.targets {
        arrows.entry(t.clone()).or_default().insert(e.id.clone());
    }
}

/// Removes an arrow from under its targets, dropping any target left with no arrows.
fn unlink(arrows: &mut BTreeMap<ElementId, BTreeSet<ElementId>>, e: &Entry) {
    for t in &e.targets {
        if let Some(set) = arrows.get_mut(t) {
            set.remove(&e.id);
            if set.is_empty() {
                arrows.remove(t);
            }
        }
    }
}

#[cfg(test)]
mod tests;
