//! Per-element hand-drawn geometry: roughr's output mapped into Placemat's op type, local to
//! the element's origin, with the hachure gap capped and a plain box standing in for any
//! generation that panics or produces a non-finite value.
#![cfg_attr(not(test), expect(dead_code, reason = "used by Scene in Task 4"))]

use std::panic::{AssertUnwindSafe, catch_unwind};

use roughr::core::{Drawable, FillStyle as RoughFill, Op as RoughOp, OpSetType, OpType, Options};
use roughr::generator::Generator;
use roughr::{Point2D, Srgba};

use super::{Geometry, Op, OpSet, Rect, SetKind, TextMeasure};
use crate::board::{Board, Boxed, Element, Fill, FillStyle, Kind, Path};

/// An element's geometry with the world origin its ops are local to and its world bounds.
#[derive(Debug)]
pub(crate) struct Built {
    pub(crate) origin: (f64, f64),
    pub(crate) bounds: Rect,
    pub(crate) geometry: Geometry,
}

/// The most hachure lines a fill may hold across its extent.
pub(crate) const HACHURE_LINES_MAX: f32 = 2000.0;

/// The fill line gap: roughr's own default of four stroke widths with its 0.1 floor, widened
/// so a huge box never holds more than [`HACHURE_LINES_MAX`] lines.
pub(crate) fn hachure_gap(stroke_width: f32, extent: f32) -> f32 {
    (4.0 * stroke_width)
        .max(0.1)
        .max(extent / HACHURE_LINES_MAX)
}

/// The jitter inputs shared by every roughr call for one element, in world units.
struct Style {
    seed: u64,
    width: f32,
    roughness: f32,
}

/// Builds the element's geometry. Text and arrows get an empty geometry until their layout
/// and routing exist.
pub(crate) fn build(e: &Element, _board: &Board, _measure: &dyn TextMeasure) -> Built {
    let style = Style {
        seed: u64::from(e.seed.get()),
        width: f32::from(e.stroke.width) / 10.0,
        roughness: f32::from(e.stroke.roughness) / 10.0,
    };
    let (origin, geometry) = match &e.kind {
        Kind::Rect(b) => boxed(b, Shape::Rect, &style),
        Kind::Ellipse(b) => boxed(b, Shape::Ellipse, &style),
        Kind::Diamond(b) => boxed(b, Shape::Diamond, &style),
        Kind::Line(p) => path(p, false, &style),
        Kind::Freedraw(p) => path(p, true, &style),
        Kind::Text(t) => return empty((f64::from(t.at.x), f64::from(t.at.y))),
        Kind::Arrow(_) => return empty((0.0, 0.0)),
    };
    let bounds = bounds(origin, &geometry, style.width);
    Built {
        origin,
        bounds,
        geometry,
    }
}

/// `generate` under `catch_unwind`, so a roughr bug costs one element its jitter rather than
/// the app; a panic gives the plain `w` x `h` box, and the result then passes [`checked`].
pub(crate) fn guarded(generate: impl FnOnce() -> Geometry, w: f32, h: f32) -> Geometry {
    match catch_unwind(AssertUnwindSafe(generate)) {
        Ok(g) => checked(g, w, h),
        Err(_) => plain_box(w, h, None),
    }
}

/// `g` when every op value is finite, else the plain `w` x `h` box marked as a fallback.
pub(crate) fn checked(g: Geometry, w: f32, h: f32) -> Geometry {
    let finite = g
        .rough
        .iter()
        .chain(&g.outline)
        .flat_map(|s| &s.ops)
        .all(|op| {
            points(op)
                .iter()
                .all(|(x, y)| x.is_finite() && y.is_finite())
        });
    if finite { g } else { plain_box(w, h, g.label) }
}

#[derive(Clone, Copy)]
enum Shape {
    Rect,
    Ellipse,
    Diamond,
}

/// A box drawn from its normalised corner: the full shape when both sides are positive, a
/// single unfilled line when one side is zero, and a dot when both are.
fn boxed(b: &Boxed, shape: Shape, style: &Style) -> ((f64, f64), Geometry) {
    let (x, y) = (f64::from(b.at.x), f64::from(b.at.y));
    let (sw, sh) = (f64::from(b.size.w), f64::from(b.size.h));
    let origin = (x.min(x + sw), y.min(y + sh));
    let (w, h) = (sw.abs() as f32, sh.abs() as f32);
    let fill = match b.fill {
        Fill::Colour { style, .. } => Some(style),
        Fill::None => None,
    };
    let geometry = if w > 0.0 && h > 0.0 {
        let options = options(style, fill, w.max(h));
        guarded(|| full_box(shape, w, h, options), w, h)
    } else if w > 0.0 || h > 0.0 {
        let options = options(style, None, 0.0);
        guarded(
            || Geometry {
                rough: sets(Generator::default().line(0.0, 0.0, w, h, &Some(options))),
                outline: vec![stroke(vec![Op::Move(0.0, 0.0), Op::Line(w, h)])],
                ..Geometry::default()
            },
            w,
            h,
        )
    } else {
        dot((0.0, 0.0), style.width)
    };
    (origin, geometry)
}

fn full_box(shape: Shape, w: f32, h: f32, options: Options) -> Geometry {
    let generator = Generator::default();
    let options = Some(options);
    let (rough, outline) = match shape {
        Shape::Rect => (
            generator.rectangle(0.0, 0.0, w, h, &options),
            rect_ops(w, h),
        ),
        Shape::Ellipse => (
            generator.ellipse(w / 2.0, h / 2.0, w, h, &options),
            ellipse_ops((w / 2.0, h / 2.0), w / 2.0, h / 2.0),
        ),
        Shape::Diamond => {
            let corners = [(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)];
            let points: Vec<Point2D<f32>> =
                corners.iter().map(|&(x, y)| Point2D::new(x, y)).collect();
            let mut ops = vec![Op::Move(w / 2.0, 0.0)];
            ops.extend(corners.iter().skip(1).map(|&(x, y)| Op::Line(x, y)));
            ops.push(Op::Line(w / 2.0, 0.0));
            (generator.polygon(&points, &options), ops)
        }
    };
    Geometry {
        rough: sets(rough),
        outline: vec![stroke(outline)],
        ..Geometry::default()
    }
}

/// A path from its origin: tenths become world units and consecutive repeats are dropped.
/// Under two vertices it is a dot; a line is roughr's linear path, a freedraw one smooth pass.
fn path(p: &Path, smooth: bool, style: &Style) -> ((f64, f64), Geometry) {
    let origin = (f64::from(p.at.x), f64::from(p.at.y));
    let mut points = p.points.clone();
    points.dedup();
    let vertices: Vec<(f32, f32)> = points
        .iter()
        .map(|q| {
            let tenth = |v: i32| (f64::from(v) / 10.0) as f32;
            (tenth(q.x), tenth(q.y))
        })
        .collect();
    let geometry = match vertices.as_slice() {
        [] => dot((0.0, 0.0), style.width),
        [only] => dot(*only, style.width),
        _ => {
            // The fallback box is the vertex AABB, so it is built at (0, 0) and moved there.
            let (x0, y0, x1, y1) = vertices.iter().fold(
                (
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ),
                |(a, b, c, d), &(x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)),
            );
            let g = guarded(|| polyline(&vertices, smooth, style), x1 - x0, y1 - y0);
            if g.fallback { shifted(g, x0, y0) } else { g }
        }
    };
    (origin, geometry)
}

fn polyline(vertices: &[(f32, f32)], smooth: bool, style: &Style) -> Geometry {
    let generator = Generator::default();
    let points: Vec<Point2D<f32>> = vertices.iter().map(|&(x, y)| Point2D::new(x, y)).collect();
    let mut options = options(style, None, 0.0);
    let rough = if smooth {
        options.roughness = Some(0.0);
        options.disable_multi_stroke = Some(true);
        generator.curve(&points, &Some(options))
    } else {
        generator.linear_path(&points, false, &Some(options))
    };
    let outline: Vec<Op> = vertices
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            if i == 0 {
                Op::Move(x, y)
            } else {
                Op::Line(x, y)
            }
        })
        .collect();
    Geometry {
        rough: sets(rough),
        outline: vec![stroke(outline)],
        ..Geometry::default()
    }
}

/// roughr options for one call: the element's jitter, a dummy colour (paint applies the
/// element's own), and the capped hachure gap on a filled shape only.
fn options(style: &Style, fill: Option<FillStyle>, extent: f32) -> Options {
    let mut options = Options {
        seed: Some(style.seed),
        stroke_width: Some(style.width),
        roughness: Some(style.roughness),
        ..Options::default()
    };
    if let Some(fill) = fill {
        options.fill = Some(Srgba::new(0.0, 0.0, 0.0, 1.0));
        options.fill_style = Some(match fill {
            FillStyle::Hachure => RoughFill::Hachure,
            FillStyle::CrossHatch => RoughFill::CrossHatch,
            FillStyle::Solid => RoughFill::Solid,
            FillStyle::Zigzag => RoughFill::ZigZag,
        });
        options.hachure_gap = Some(hachure_gap(style.width, extent));
    }
    options
}

fn sets(d: Drawable<f32>) -> Vec<OpSet> {
    d.sets
        .iter()
        .map(|s| OpSet {
            kind: match s.op_set_type {
                OpSetType::Path => SetKind::Stroke,
                OpSetType::FillPath => SetKind::Fill,
                OpSetType::FillSketch => SetKind::Sketch,
            },
            ops: s.ops.iter().map(op).collect(),
        })
        .collect()
}

/// One roughr op by its data's shape; a malformed one becomes NaN so [`checked`] rejects it.
fn op(o: &RoughOp<f32>) -> Op {
    match (&o.op, o.data.as_slice()) {
        (OpType::Move, &[x, y]) => Op::Move(x, y),
        (OpType::LineTo, &[x, y]) => Op::Line(x, y),
        (OpType::BCurveTo, &[a, b, c, d, e, f]) => Op::Cubic([a, b, c, d, e, f]),
        _ => Op::Move(f32::NAN, f32::NAN),
    }
}

fn stroke(ops: Vec<Op>) -> OpSet {
    OpSet {
        kind: SetKind::Stroke,
        ops,
    }
}

/// The closed `w` x `h` box from (0, 0), clockwise.
fn rect_ops(w: f32, h: f32) -> Vec<Op> {
    vec![
        Op::Move(0.0, 0.0),
        Op::Line(w, 0.0),
        Op::Line(w, h),
        Op::Line(0.0, h),
        Op::Line(0.0, 0.0),
    ]
}

/// An ellipse as four quarter-arc cubics, starting at its rightmost point.
fn ellipse_ops((cx, cy): (f32, f32), rx: f32, ry: f32) -> Vec<Op> {
    const K: f32 = 0.552_284_8;
    let (kx, ky) = (K * rx, K * ry);
    vec![
        Op::Move(cx + rx, cy),
        Op::Cubic([cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry]),
        Op::Cubic([cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy]),
        Op::Cubic([cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry]),
        Op::Cubic([cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy]),
    ]
}

/// A plain round dot at `centre`, the same set drawn rough and as the outline.
fn dot(centre: (f32, f32), stroke_width: f32) -> Geometry {
    let r = stroke_width.max(1.0) / 4.0;
    let set = stroke(ellipse_ops(centre, r, r));
    Geometry {
        rough: vec![set.clone()],
        outline: vec![set],
        ..Geometry::default()
    }
}

fn plain_box(w: f32, h: f32, label: Option<super::Label>) -> Geometry {
    let set = stroke(rect_ops(w, h));
    Geometry {
        rough: vec![set.clone()],
        outline: vec![set],
        label,
        fallback: true,
    }
}

fn empty(at: (f64, f64)) -> Built {
    Built {
        origin: at,
        bounds: Rect::new(at.0, at.1, at.0, at.1),
        geometry: Geometry::default(),
    }
}

/// The coordinate pairs an op touches, control points included.
fn points(op: &Op) -> Vec<(f32, f32)> {
    match *op {
        Op::Move(x, y) | Op::Line(x, y) => vec![(x, y)],
        Op::Cubic([a, b, c, d, e, f]) => vec![(a, b), (c, d), (e, f)],
    }
}

fn shifted(mut g: Geometry, dx: f32, dy: f32) -> Geometry {
    for set in g.rough.iter_mut().chain(&mut g.outline) {
        for op in &mut set.ops {
            *op = match *op {
                Op::Move(x, y) => Op::Move(x + dx, y + dy),
                Op::Line(x, y) => Op::Line(x + dx, y + dy),
                Op::Cubic([a, b, c, d, e, f]) => {
                    Op::Cubic([a + dx, b + dy, c + dx, d + dy, e + dx, f + dy])
                }
            };
        }
    }
    g
}

/// The world AABB of every op coordinate, padded by half the stroke width.
fn bounds(origin: (f64, f64), g: &Geometry, stroke_width: f32) -> Rect {
    let all = g.rough.iter().chain(&g.outline).flat_map(|s| &s.ops);
    let (x0, y0, x1, y1) = all.flat_map(points).fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |(a, b, c, d), (x, y)| {
            let (x, y) = (f64::from(x), f64::from(y));
            (a.min(x), b.min(y), c.max(x), d.max(y))
        },
    );
    if x0 > x1 {
        return Rect::new(origin.0, origin.1, origin.0, origin.1);
    }
    let pad = f64::from(stroke_width) / 2.0;
    Rect::new(
        origin.0 + x0 - pad,
        origin.1 + y0 - pad,
        origin.0 + x1 + pad,
        origin.1 + y1 + pad,
    )
}

#[cfg(test)]
mod tests;
