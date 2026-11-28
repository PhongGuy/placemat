//! The camera that maps world units to screen pixels, and the pure rules a renderer applies at
//! a given zoom: how much detail an element gets, which label size to draw, and the dash pattern.

use super::Rect;
use crate::board::Dash;

/// The view onto the board: `screen = (world - origin) * zoom`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The world point at the screen's top-left corner.
    pub origin: (f64, f64),
    /// Screen pixels per world unit, within [`Camera::MIN_ZOOM`, `Camera::MAX_ZOOM`].
    pub zoom: f64,
}

impl Camera {
    /// The farthest a camera zooms out.
    pub const MIN_ZOOM: f64 = 0.1;
    /// The farthest a camera zooms in.
    pub const MAX_ZOOM: f64 = 30.0;
    /// The clear border [`Camera::fit`] leaves around the content, in logical pixels (the
    /// window's own units, never scaled by the display's scale factor).
    pub const FIT_MARGIN: f64 = 32.0;

    /// A camera at `origin` with `zoom` clamped to the allowed range; a non-finite zoom is 1.
    pub fn new(origin: (f64, f64), zoom: f64) -> Self {
        Self {
            origin,
            zoom: clamp_zoom(zoom),
        }
    }

    /// The screen position of a world point.
    pub fn to_screen(self, world: (f64, f64)) -> (f64, f64) {
        (
            (world.0 - self.origin.0) * self.zoom,
            (world.1 - self.origin.1) * self.zoom,
        )
    }

    /// The world point under a screen position.
    pub fn to_world(self, screen: (f64, f64)) -> (f64, f64) {
        (
            screen.0 / self.zoom + self.origin.0,
            screen.1 / self.zoom + self.origin.1,
        )
    }

    /// Moves the content by a screen-space drag; a non-finite delta is ignored.
    pub fn pan(&mut self, screen_dx: f64, screen_dy: f64) {
        if !(screen_dx.is_finite() && screen_dy.is_finite()) {
            return;
        }
        self.origin.0 -= screen_dx / self.zoom;
        self.origin.1 -= screen_dy / self.zoom;
    }

    /// Scales the zoom by `factor`, clamped, keeping the world point under `screen` fixed.
    /// A factor that is not finite and positive, or a non-finite `screen`, is ignored.
    pub fn zoom_about(&mut self, screen: (f64, f64), factor: f64) {
        if !(factor.is_finite() && factor > 0.0 && screen.0.is_finite() && screen.1.is_finite()) {
            return;
        }
        let world = self.to_world(screen);
        self.zoom = clamp_zoom(self.zoom * factor);
        self.origin = (
            world.0 - screen.0 / self.zoom,
            world.1 - screen.1 / self.zoom,
        );
    }

    /// The world rectangle a viewport of the given pixel size shows.
    pub fn view(self, viewport: (f64, f64)) -> Rect {
        let far = self.to_world(viewport);
        Rect::new(self.origin.0, self.origin.1, far.0, far.1)
    }

    /// The camera that shows all of `content` centred in `viewport` with [`Camera::FIT_MARGIN`]
    /// logical pixels clear on each side, shrunk to a quarter of the shorter viewport side when
    /// the viewport is small. Zoom is the smallest inner-to-content ratio over non-zero sides (1
    /// for a point), clamped. A viewport side that is not finite and positive gives zoom 1
    /// centred on the content.
    pub fn fit(content: Rect, viewport: (f64, f64)) -> Camera {
        let centre = (
            (content.x0 + content.x1) / 2.0,
            (content.y0 + content.y1) / 2.0,
        );
        let usable = |side: f64| side.is_finite() && side > 0.0;
        if !(usable(viewport.0) && usable(viewport.1)) {
            return Camera {
                origin: centre,
                zoom: 1.0,
            };
        }
        // The margin is in screen pixels, so it narrows the viewport before the ratio is taken;
        // centring against the full viewport then also holds when the zoom is clamped.
        let margin = Self::FIT_MARGIN.min(viewport.0.min(viewport.1) / 4.0);
        let ratios = [
            (viewport.0 - 2.0 * margin, content.width()),
            (viewport.1 - 2.0 * margin, content.height()),
        ];
        let zoom = ratios
            .iter()
            .filter(|(_, side)| *side > 0.0)
            .map(|(inner, side)| inner / side)
            .reduce(f64::min)
            .unwrap_or(1.0);
        let zoom = clamp_zoom(zoom);
        Camera {
            origin: (
                centre.0 - viewport.0 / (2.0 * zoom),
                centre.1 - viewport.1 / (2.0 * zoom),
            ),
            zoom,
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new((0.0, 0.0), 1.0)
    }
}

fn clamp_zoom(zoom: f64) -> f64 {
    if zoom.is_finite() {
        zoom.clamp(Camera::MIN_ZOOM, Camera::MAX_ZOOM)
    } else {
        1.0
    }
}

/// How much of an element to draw, by its on-screen extent.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// The full hand-drawn geometry and label.
    Full,
    /// A plain outline.
    Simple,
    /// A single dot.
    Dot,
}

/// Elements smaller than this many pixels draw as a dot.
pub const DOT_BELOW_PX: f32 = 2.0;
/// Elements smaller than this many pixels draw as a plain outline.
pub const SIMPLE_BELOW_PX: f32 = 24.0;

/// The detail level for an element whose largest on-screen side is `px_extent`; NaN is a dot.
pub fn detail(px_extent: f32) -> Detail {
    if px_extent.is_nan() || px_extent < DOT_BELOW_PX {
        Detail::Dot
    } else if px_extent < SIMPLE_BELOW_PX {
        Detail::Simple
    } else {
        Detail::Full
    }
}

/// Labels smaller than this many pixels are not drawn.
pub const LABEL_MIN_PX: f32 = 4.0;
/// The largest label size drawn, in pixels.
pub const LABEL_MAX_PX: f32 = 256.0;

/// The pixel size to draw a label of `world_size` at `zoom`, or `None` when it would be under
/// [`LABEL_MIN_PX`] or is NaN. Sizes snap to quarter octaves, so zooming reuses few glyph sizes.
pub fn label_px(world_size: f32, zoom: f64) -> Option<f32> {
    let px = (f64::from(world_size) * zoom) as f32;
    if px.is_nan() || px < LABEL_MIN_PX {
        return None;
    }
    let k = (4.0 * px.log2()).round();
    Some(2f32.powf(k / 4.0).min(LABEL_MAX_PX))
}

/// The most dash periods a path is drawn with before it falls back to solid.
pub const DASHES_MAX: f32 = 2000.0;

/// The `[on, off]` dash pattern in pixels for a stroke `width_px` wide along a path `path_px`
/// long, scaled from Excalidraw's patterns. `None` means draw solid: a solid dash, a non-finite
/// input, or a path that would need more than [`DASHES_MAX`] periods.
pub fn dash_array(dash: Dash, width_px: f32, path_px: f32) -> Option<[f32; 2]> {
    if !(width_px.is_finite() && path_px.is_finite()) {
        return None;
    }
    let w = width_px.max(1.0);
    let pattern = match dash {
        Dash::Solid => return None,
        Dash::Dashed => [4.0 * w, 5.0 * w],
        Dash::Dotted => [0.75 * w, 4.0 * w],
    };
    let [on, off] = pattern;
    (path_px / (on + off) <= DASHES_MAX).then_some(pattern)
}
