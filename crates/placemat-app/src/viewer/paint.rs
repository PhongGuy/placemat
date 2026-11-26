//! The pure half of painting: colours, stroke widths, the world-to-canvas map and which op sets
//! each level of detail paints.

use gpui_kit::Rgba;
use placemat_core::board::Colour;
use placemat_core::scene::{Camera, Detail, Geometry, Op, OpSet, SetKind};

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
    // Walk the polygon in f64 screen space; a Move starts a new subpath without adding length.
    let mut at = (0.0, 0.0);
    let mut len = 0.0;
    let mut step = |to: (f64, f64), at: &mut (f64, f64)| {
        len += (to.0 - at.0).hypot(to.1 - at.1);
        *at = to;
    };
    for op in ops {
        match *op {
            Op::Move(x, y) => at = to_screen((x, y), origin, camera),
            Op::Line(x, y) => step(to_screen((x, y), origin, camera), &mut at),
            Op::Cubic([ax, ay, bx, by, x, y]) => {
                for p in [(ax, ay), (bx, by), (x, y)] {
                    step(to_screen(p, origin, camera), &mut at);
                }
            }
            _ => {}
        }
    }
    len as f32
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
