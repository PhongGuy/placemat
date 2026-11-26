//! The pure half of painting: colours, stroke widths, the world-to-canvas map and which op sets
//! each level of detail paints.

use gpui_kit::Rgba;
use placemat_core::board::Colour;
use placemat_core::scene::{Camera, Detail, Geometry, Op, OpSet, Rect, SetKind};

use crate::viewer::budget::cubic_curves;

/// `c` with each channel scaled from 0..=255 to 0..=1.
pub fn rgba(c: Colour) -> Rgba {
    let [r, g, b, a] = c.channels().map(|v| f32::from(v) / 255.0);
    Rgba { r, g, b, a }
}

/// A stroke `width_tenths` tenths of a world unit wide, in canvas pixels at `zoom`; never under 1.
pub fn stroke_px(width_tenths: u16, zoom: f64) -> f32 {
    (f64::from(width_tenths) / 10.0 * zoom).max(1.0) as f32
}

/// A sketch (hachure) line, half the stroke width as roughr draws it; never under 0.5 px.
pub fn sketch_px(width_tenths: u16, zoom: f64) -> f32 {
    (f64::from(width_tenths) / 20.0 * zoom).max(0.5) as f32
}

/// A local op point to canvas pixels: (origin + local - camera.origin) * zoom, computed in f64.
pub fn to_px(local: (f32, f32), origin: (f64, f64), camera: Camera) -> (f32, f32) {
    let (x, y) = to_screen(local, origin, camera);
    (x as f32, y as f32)
}

/// Summed screen length of the set's control polygon, from each Move.
pub fn screen_len(ops: &[Op], origin: (f64, f64), camera: Camera) -> f32 {
    walk(ops, origin, camera, 1.0).len as f32
}

/// A set's control polygon on screen, in canvas logical pixels, from one walk over its ops.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    /// The summed length, from each Move.
    pub len: f64,
    /// The summed L1 length: the grid lines a path along it crosses per pixel of cell.
    pub l1: f64,
    /// The box around every point, None for no ops.
    pub bounds: Option<Rect>,
    /// The most curves GPUI's fill builder makes of the set: each contour's closing line,
    /// one per line and `cubic_curves` per cubic.
    pub fill_curves: u64,
    /// The lines a stroke of the set is flattened into: one per line, `cubic_segments` per
    /// cubic.
    pub segments: u64,
    /// The subpaths: one per Move.
    pub subpaths: u64,
}

/// Walks `ops` once in f64 screen space at device `scale`; a Move starts a new subpath
/// without adding length.
pub fn walk(ops: &[Op], origin: (f64, f64), camera: Camera, scale: f64) -> Walk {
    let mut w = Walk {
        len: 0.0,
        l1: 0.0,
        bounds: None,
        fill_curves: 0,
        segments: 0,
        subpaths: 0,
    };
    let mut at = (0.0, 0.0);
    let visit = |p: (f64, f64), w: &mut Walk| {
        let b = w.bounds.get_or_insert(Rect::new(p.0, p.1, p.0, p.1));
        *b = Rect::new(b.x0.min(p.0), b.y0.min(p.1), b.x1.max(p.0), b.y1.max(p.1));
    };
    let step = |to: (f64, f64), at: &mut (f64, f64), w: &mut Walk| -> f64 {
        let d = (to.0 - at.0).hypot(to.1 - at.1);
        w.len += d;
        w.l1 += (to.0 - at.0).abs() + (to.1 - at.1).abs();
        *at = to;
        d
    };
    for op in ops {
        match *op {
            Op::Move(x, y) => {
                at = to_screen((x, y), origin, camera);
                visit(at, &mut w);
                w.fill_curves = w.fill_curves.saturating_add(1);
                w.subpaths += 1;
            }
            Op::Line(x, y) => {
                let to = to_screen((x, y), origin, camera);
                step(to, &mut at, &mut w);
                visit(to, &mut w);
                w.fill_curves = w.fill_curves.saturating_add(1);
                w.segments += 1;
            }
            Op::Cubic([ax, ay, bx, by, x, y]) => {
                let start = (at.0 as f32, at.1 as f32);
                let points = [(ax, ay), (bx, by), (x, y)].map(|p| to_px(p, origin, camera));
                let n = cubic_segments([start, points[0], points[1], points[2]], scale);
                w.segments += u64::from(n);
                let mut polygon = 0.0;
                for p in [(ax, ay), (bx, by), (x, y)] {
                    let to = to_screen(p, origin, camera);
                    polygon += step(to, &mut at, &mut w);
                    visit(to, &mut w);
                }
                w.fill_curves = w.fill_curves.saturating_add(cubic_curves(polygon as f32));
            }
            _ => {}
        }
    }
    w
}

/// How many lines a stroked cubic with these canvas-pixel control points is flattened into at
/// `scale`: enough to stay within about 1.5 device pixels, at most 32.
pub fn cubic_segments(p: [(f32, f32); 4], scale: f64) -> u32 {
    // n lines stray at most |B''| / (8 n^2) = 6 |d| / (8 n^2), |d| the larger second difference.
    // n solves |d| / (8 n^2) = 0.25 without the 6, so the bound is 1.5 px; pricing relies on it.
    const TOLERANCE_PX: f64 = 0.25;
    let [a, b, c, e] = p.map(|(x, y)| (f64::from(x) * scale, f64::from(y) * scale));
    let second = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
        (p.0 - 2.0 * q.0 + r.0).hypot(p.1 - 2.0 * q.1 + r.1)
    };
    let d = second(a, b, c).max(second(b, c, e));
    let n = (d / (8.0 * TOLERANCE_PX)).sqrt().ceil();
    if n.is_finite() {
        n.clamp(1.0, 32.0) as u32
    } else {
        32
    }
}

/// The sets painted at `detail`: Full all `rough`, Simple the `outline`'s Stroke sets, Dot none.
pub fn painted(g: &Geometry, detail: Detail) -> impl Iterator<Item = &OpSet> {
    let sets: Box<dyn Iterator<Item = &OpSet>> = match detail {
        Detail::Full => Box::new(g.rough.iter()),
        Detail::Simple => Box::new(g.outline.iter().filter(|s| is_stroke(s.kind))),
        Detail::Dot | _ => Box::new(std::iter::empty()),
    };
    sets
}

fn is_stroke(kind: SetKind) -> bool {
    match kind {
        SetKind::Stroke => true,
        SetKind::Fill | SetKind::Sketch | _ => false,
    }
}

fn to_screen(local: (f32, f32), origin: (f64, f64), camera: Camera) -> (f64, f64) {
    camera.to_screen((origin.0 + f64::from(local.0), origin.1 + f64::from(local.1)))
}
