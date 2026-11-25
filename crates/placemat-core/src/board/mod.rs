//! Placemat boards: the typed element model a `.board` file holds.
//!
//! Ids, seeds, z keys and colours check their format when built. Coordinates, tenths and
//! anchors are plain numbers; [`Board`]'s mutators check them and every limit in [`LIMITS`].

mod escape;
mod load;
mod model;
mod rng;
mod write;
mod zkey;

use std::collections::BTreeMap;

pub use load::{Access, End, LoadError, Loaded, Warning, load};
pub use model::{
    Align, Anchor, Arrow, ArrowEnd, Boxed, Colour, Dash, Element, ElementId, Fill, FillStyle, Font,
    FontFamily, Head, Heads, Kind, Path, Point, Seed, Size, Stroke, Text,
};
pub use rng::Rng;
pub use write::{WriteError, write};
pub use zkey::ZKey;

/// The board format version this build reads and writes.
pub const VERSION: u32 = 0;

/// The bounds every board stays within, so a hostile or runaway file cannot exhaust memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Largest file read or written, in bytes.
    pub hard_bytes: usize,
    /// File size above which a load warns that the board is too large to share comfortably.
    pub team_bytes: usize,
    /// Most elements on a board.
    pub elements: usize,
    /// Most line and freedraw points on a board.
    pub points_total: usize,
    /// Most points in one element.
    pub points_per_element: usize,
    /// Most label and content bytes on a board, counted after escaping.
    pub string_bytes_total: usize,
    /// Most bytes in one label or content, counted after escaping.
    pub string_bytes_each: usize,
    /// Most bytes of unknown data written for a board.
    pub unknown_bytes_total: usize,
}

/// The one limit set the loader, the writer and every mutator enforce.
pub const LIMITS: Limits = Limits {
    hard_bytes: 16 << 20,
    team_bytes: 1 << 20,
    elements: 10_000,
    points_total: 250_000,
    points_per_element: 10_000,
    string_bytes_total: 1 << 20,
    string_bytes_each: 64 << 10,
    unknown_bytes_total: 1 << 20,
};

/// An edit refused because it would break the board's limits. `limit` names the [`Limits`] field it
/// would exceed, or is `"range"` when a value lies outside its format (coordinates, tenths, anchors).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("board {limit} limit reached")]
#[non_exhaustive]
pub struct LimitReached {
    /// The exceeded [`Limits`] field, or `"range"`.
    pub limit: &'static str,
}

impl LimitReached {
    const fn new(limit: &'static str) -> Self {
        Self { limit }
    }
}

/// Largest magnitude of a coordinate or size, in whole units.
const COORD_MAX: i32 = 1 << 24;
/// Largest magnitude of a path point, in tenths.
const POINT_MAX: i32 = 10 * COORD_MAX;
/// Largest stroke width and font size, in tenths.
const TENTHS_MAX: u16 = 1000;
/// Largest roughness, in tenths.
const ROUGHNESS_MAX: u16 = 100;
/// Largest anchor fraction, in units of 1e-4.
const ANCHOR_MAX: u16 = 10_000;

/// What one element counts against the board totals.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Usage {
    /// Line and freedraw points.
    pub(crate) points: usize,
    /// Label and content bytes after escaping.
    pub(crate) string_bytes: usize,
    /// Unknown-data bytes as written.
    pub(crate) unknown_bytes: usize,
}

/// The element's usage, or the limit it breaks on its own: a value outside its format, or
/// more points or string bytes than one element may hold.
pub(crate) fn check(e: &Element) -> Result<Usage, LimitReached> {
    let point_ok = |p: &Point, max: i32| (-max..=max).contains(&p.x) && (-max..=max).contains(&p.y);
    let size_ok = |s: &Size| point_ok(&Point::new(s.w, s.h), COORD_MAX);
    let end_ok = |end: &ArrowEnd| match end {
        ArrowEnd::Bound { anchor, .. } => anchor.x <= ANCHOR_MAX && anchor.y <= ANCHOR_MAX,
        ArrowEnd::Free(p) => point_ok(p, COORD_MAX),
    };
    let in_range = e.stroke.width <= TENTHS_MAX
        && e.stroke.roughness <= ROUGHNESS_MAX
        && match &e.kind {
            Kind::Rect(b) | Kind::Ellipse(b) | Kind::Diamond(b) => {
                point_ok(&b.at, COORD_MAX) && size_ok(&b.size)
            }
            Kind::Line(p) | Kind::Freedraw(p) => {
                point_ok(&p.at, COORD_MAX) && p.points.iter().all(|q| point_ok(q, POINT_MAX))
            }
            Kind::Arrow(a) => end_ok(&a.from) && end_ok(&a.to),
            Kind::Text(t) => point_ok(&t.at, COORD_MAX) && t.font.size <= TENTHS_MAX,
        };
    if !in_range {
        return Err(LimitReached::new("range"));
    }
    let usage = usage(e);
    if usage.points > LIMITS.points_per_element {
        return Err(LimitReached::new("points_per_element"));
    }
    if usage.string_bytes > LIMITS.string_bytes_each {
        return Err(LimitReached::new("string_bytes_each"));
    }
    Ok(usage)
}

/// What the element counts against the board totals; an element has at most one string.
pub(crate) fn usage(e: &Element) -> Usage {
    let (points, string) = match &e.kind {
        Kind::Rect(b) | Kind::Ellipse(b) | Kind::Diamond(b) => (0, b.label.as_deref()),
        Kind::Line(p) | Kind::Freedraw(p) => (p.points.len(), None),
        Kind::Arrow(a) => (0, a.label.as_deref()),
        Kind::Text(t) => (0, Some(t.content.as_str())),
    };
    Usage {
        points,
        string_bytes: string.map_or(0, escape::escaped_len),
        unknown_bytes: write::unknown_written(e),
    }
}

/// A whiteboard: elements keyed by id, with running totals kept in step with every edit so
/// each limit check is constant work.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Board {
    version: u32,
    pub(crate) elements: BTreeMap<ElementId, Element>,
    /// Unknown header entries, in `write::entry_text` form and source order.
    pub(crate) header: Vec<String>,
    /// Unknown top-level nodes keyed by their first string argument, each in `write::node_text`
    /// form, same-key nodes in source order.
    pub(crate) keyed: BTreeMap<String, Vec<String>>,
    /// Unknown top-level nodes without a key, in source order.
    pub(crate) unkeyed: Vec<String>,
    points_total: usize,
    string_bytes_total: usize,
    /// Unknown bytes as written, the board's own and its elements'.
    unknown_bytes_total: usize,
}

impl Board {
    /// An empty board at [`VERSION`].
    pub fn new() -> Self {
        Self {
            version: VERSION,
            ..Self::default()
        }
    }

    /// The format version the board was read at.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// How many elements the board holds.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Whether the board holds no element.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// The element with this id.
    pub fn get(&self, id: &ElementId) -> Option<&Element> {
        self.elements.get(id)
    }

    /// Every element, in id order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.values()
    }

    /// Whether `id` is taken on this board, by an element or by an unknown node keyed by it.
    pub fn contains_id(&self, id: &str) -> bool {
        self.keyed.contains_key(id)
            || ElementId::new(id).is_some_and(|id| self.elements.contains_key(&id))
    }

    /// A board at `version` holding unknown data of its own, or `None` when that data alone
    /// writes over [`LIMITS`]`.unknown_bytes_total`.
    pub(crate) fn with_unknown(
        version: u32,
        header: Vec<String>,
        keyed: BTreeMap<String, Vec<String>>,
        unkeyed: Vec<String>,
    ) -> Option<Self> {
        let unknown_bytes_total = write::board_unknown_written(&header, &keyed, &unkeyed);
        (unknown_bytes_total <= LIMITS.unknown_bytes_total).then(|| Self {
            version,
            header,
            keyed,
            unkeyed,
            unknown_bytes_total,
            ..Self::default()
        })
    }

    /// Inserts the element, replacing any element with the same id. Refuses it, leaving the board
    /// unchanged, when it would break a limit in [`LIMITS`] or hold a value outside its format.
    pub fn insert(&mut self, e: Element) -> Result<(), LimitReached> {
        let new = check(&e)?;
        // Totals are figured as if the replaced element were already gone, so an edit that
        // keeps a full board at its limit still fits.
        let old = match self.elements.get(&e.id) {
            Some(old) => usage(old),
            None if self.elements.len() >= LIMITS.elements => {
                return Err(LimitReached::new("elements"));
            }
            None => Usage::default(),
        };
        let points_total = self.points_total - old.points + new.points;
        if points_total > LIMITS.points_total {
            return Err(LimitReached::new("points_total"));
        }
        let string_bytes_total = self.string_bytes_total - old.string_bytes + new.string_bytes;
        if string_bytes_total > LIMITS.string_bytes_total {
            return Err(LimitReached::new("string_bytes_total"));
        }
        let unknown_bytes_total = self.unknown_bytes_total - old.unknown_bytes + new.unknown_bytes;
        if unknown_bytes_total > LIMITS.unknown_bytes_total {
            return Err(LimitReached::new("unknown_bytes_total"));
        }
        self.points_total = points_total;
        self.string_bytes_total = string_bytes_total;
        self.unknown_bytes_total = unknown_bytes_total;
        self.elements.insert(e.id.clone(), e);
        Ok(())
    }

    /// Removes and returns the element with this id.
    pub fn remove(&mut self, id: &ElementId) -> Option<Element> {
        let e = self.elements.remove(id)?;
        let usage = usage(&e);
        self.points_total -= usage.points;
        self.string_bytes_total -= usage.string_bytes;
        self.unknown_bytes_total -= usage.unknown_bytes;
        Some(e)
    }

    /// Replaces the points of a line or freedraw element, under the same limits as `insert`. A missing
    /// id or an element of another kind is left alone and returns `Ok(())`.
    pub fn set_points(&mut self, id: &ElementId, points: Vec<Point>) -> Result<(), LimitReached> {
        self.edit(id, |kind| match kind {
            Kind::Line(p) | Kind::Freedraw(p) => {
                p.points = points;
                true
            }
            Kind::Rect(_)
            | Kind::Ellipse(_)
            | Kind::Diamond(_)
            | Kind::Arrow(_)
            | Kind::Text(_) => false,
        })
    }

    /// Replaces the label of a rect, ellipse, diamond or arrow, under the same limits as `insert`.
    /// A missing id or an element of another kind is left alone and returns `Ok(())`.
    pub fn set_label(&mut self, id: &ElementId, label: Option<String>) -> Result<(), LimitReached> {
        self.edit(id, |kind| match kind {
            Kind::Rect(b) | Kind::Ellipse(b) | Kind::Diamond(b) => {
                b.label = label;
                true
            }
            Kind::Arrow(a) => {
                a.label = label;
                true
            }
            Kind::Line(_) | Kind::Freedraw(_) | Kind::Text(_) => false,
        })
    }

    /// Replaces the content of a text, under the same limits as `insert`. A missing id or an
    /// element of another kind is left alone and returns `Ok(())`.
    pub fn set_content(&mut self, id: &ElementId, content: String) -> Result<(), LimitReached> {
        self.edit(id, |kind| match kind {
            Kind::Text(t) => {
                t.content = content;
                true
            }
            Kind::Rect(_)
            | Kind::Ellipse(_)
            | Kind::Diamond(_)
            | Kind::Line(_)
            | Kind::Arrow(_)
            | Kind::Freedraw(_) => false,
        })
    }

    /// Every element in paint order: by z key, ties broken by id.
    pub fn paint_order(&self) -> impl Iterator<Item = &Element> {
        let mut order: Vec<&Element> = self.elements.values().collect();
        order.sort_by(|a, b| (&a.z, &a.id).cmp(&(&b.z, &b.id)));
        order.into_iter()
    }

    /// Applies `change` to a copy of the element and inserts the copy, so a refused edit leaves
    /// the board as it was. `change` returns false when the kind lacks the field.
    fn edit(
        &mut self,
        id: &ElementId,
        change: impl FnOnce(&mut Kind) -> bool,
    ) -> Result<(), LimitReached> {
        let Some(e) = self.elements.get(id) else {
            return Ok(());
        };
        let mut copy = e.clone();
        if !change(&mut copy.kind) {
            return Ok(());
        }
        self.insert(copy)
    }
}
