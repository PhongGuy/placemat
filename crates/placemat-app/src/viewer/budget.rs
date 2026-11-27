//! The per-frame instance budget: how much detail and which labels a frame can afford before
//! GPUI's instance buffer overflows.

use placemat_core::scene::{Detail, LabelLine, Rect};

/// The most label glyphs one frame paints: 7 MiB of 112-byte sprites.
pub const GLYPHS_MAX: usize = 65_536;
/// The estimated path bytes one frame paints: half of GPUI's 128 MiB instance cap, so paths,
/// glyphs, dots and chrome together still leave headroom for the estimate's tile count.
pub const PATH_BYTES_MAX: u64 = 64 << 20;
/// The most curves one flattened line segment becomes once stroked with bevel joins: two
/// offset lines, the outer bevel and the inner pivot pair.
pub const CURVES_PER_SEGMENT: u64 = 5;
/// The most curves each open subpath adds once stroked: two butt caps and the close.
pub const CURVES_PER_SUBPATH: u64 = 3;

/// Glyph slots one tag's background quad is charged: two 112-byte sprites hold a 160-byte quad.
pub const QUAD_GLYPHS: usize = 2;

/// The non-whitespace chars of a label, one glyph sprite each, plus [`QUAD_GLYPHS`] for each
/// tag's background quad.
pub fn label_cost(lines: &[LabelLine]) -> usize {
    lines
        .iter()
        .map(|l| {
            let glyphs = l.text.chars().filter(|c| !c.is_whitespace()).count();
            glyphs.saturating_add(l.hidden.len().saturating_mul(QUAD_GLYPHS))
        })
        .fold(0, usize::saturating_add)
}

/// The most curves a filled cubic of control polygon `len_px` logical pixels becomes.
pub fn cubic_curves(len_px: f32) -> u64 {
    // GPUI (through lyon) splits a cubic into ceil(cbrt(|d| / (sqrt(432) * 0.25)))
    // quadratics, |d| at most twice the polygon, and each into at most three monotone pieces.
    const TOLERANCE_PX: f64 = 0.25;
    let quads = (2.0 * f64::from(len_px) / (432.0_f64.sqrt() * TOLERANCE_PX))
        .cbrt()
        .ceil();
    if !quads.is_finite() {
        return u64::MAX;
    }
    (quads as u64).saturating_add(1).saturating_mul(3)
}

/// The most curves a stroked set of `segments` flattened line segments in `subpaths`
/// subpaths becomes, cut into `dashes` dash periods (0 when solid).
pub fn stroke_curves(segments: u64, subpaths: u64, dashes: f32) -> u64 {
    if !dashes.is_finite() {
        return u64::MAX;
    }
    let pieces = dash_pieces(subpaths, dashes);
    CURVES_PER_SEGMENT
        .saturating_mul(segments.saturating_add(pieces))
        .saturating_add(CURVES_PER_SUBPATH.saturating_mul(subpaths.saturating_add(pieces)))
}

/// The dashes a dashed stroke may start: each dash starts a subpath and may cut a segment in
/// two, and a subpath may also open on a partial dash. 0 when solid.
fn dash_pieces(subpaths: u64, dashes: f32) -> u64 {
    if dashes > 0.0 {
        (f64::from(dashes).ceil() as u64).saturating_add(subpaths)
    } else {
        0
    }
}

/// The L1 length of a stroke's outline in device pixels, for the grid lines it crosses: both
/// offset sides of the centre line `l1`, plus up to three `width`-long lines per join (bevel
/// and inner pivot pair) and per subpath (two caps, each at most sqrt(2) width in L1).
pub fn stroke_l1(l1: f64, width: f64, segments: u64, subpaths: u64, dashes: f32) -> f64 {
    let pieces = dash_pieces(subpaths, dashes) as f64;
    let joins = segments as f64 + pieces;
    let ends = subpaths as f64 + pieces;
    2.0 * l1 + width * (3.0 * joins + 3.0 * ends)
}

/// The most cells GPUI's binner lays over one path.
const GRID_CELLS_MAX: f64 = 4096.0;

/// The smallest cell size GPUI's binner may choose for a `w` by `h` device-pixel path.
pub fn cell_min(w: f64, h: f64) -> f64 {
    // GPUI's `cell_size` starts here and only ever picks this size or a larger one.
    let cells = |c: f64| ((w / c).floor() + 1.0) * ((h / c).floor() + 1.0);
    let mut c = 16.0;
    while cells(c) > GRID_CELLS_MAX && c.is_finite() {
        c *= 2.0;
    }
    c
}

/// The estimated instance bytes of a path of `curves` curves spanning `bounds` (device pixels,
/// L1 length `l1`) painted under `clip`; `u64::MAX` when any input is not finite.
pub fn path_cost(curves: u64, bounds: Rect, l1: f64, clip: Rect) -> u64 {
    let all = [bounds.x0, bounds.y0, bounds.x1, bounds.y1, l1];
    if !all.iter().all(|v| v.is_finite()) {
        return u64::MAX;
    }
    // Every curve is listed in the cells it crosses, uploaded whether visible or not; only
    // the tiles inside the clip survive, and never more than the grid holds.
    let c = cell_min(bounds.width(), bounds.height());
    let listings = (curves as f64) * 3.0 + (l1 / c).ceil();
    let span = |lo: f64, hi: f64, clip_lo: f64, clip_hi: f64| {
        let (a, b) = (lo.max(clip_lo), hi.min(clip_hi));
        if a > b || (a == b && lo < hi) {
            0.0
        } else {
            ((b - lo) / c).floor() - ((a - lo) / c).floor() + 1.0
        }
    };
    let tiles = (span(bounds.x0, bounds.x1, clip.x0, clip.x1)
        * span(bounds.y0, bounds.y1, clip.y0, clip.y1))
    .min(GRID_CELLS_MAX);
    let bytes = 112.0 + 40.0 * listings + 56.0 * tiles;
    if bytes.is_finite() {
        bytes as u64
    } else {
        u64::MAX
    }
}

/// What one visible item would cost to paint.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cost {
    /// The detail its size on screen calls for.
    pub natural: Detail,
    /// Path bytes at `Full` detail.
    pub full: u64,
    /// Path bytes at `Simple` detail.
    pub simple: u64,
    /// Glyphs of its label; 0 when it paints none.
    pub glyphs: usize,
}

impl Cost {
    /// The cost of an item at `natural` detail.
    pub fn new(natural: Detail, full: u64, simple: u64, glyphs: usize) -> Self {
        Self {
            natural,
            full,
            simple,
            glyphs,
        }
    }
}

/// What one item may paint this frame.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grant {
    /// The detail to paint at.
    pub detail: Detail,
    /// Whether to paint its label.
    pub label: bool,
}

impl Grant {
    /// A grant of `detail`, with or without the label.
    pub fn new(detail: Detail, label: bool) -> Self {
        Self { detail, label }
    }
}

/// Whether `grant` paints the item as its size on screen would: its natural detail, with its
/// label unless it is a dot.
pub fn whole(cost: &Cost, grant: &Grant) -> bool {
    let label = cost.glyphs > 0 && cost.natural != Detail::Dot;
    grant.detail == cost.natural && grant.label == label
}

/// One grant per cost, in the same (paint) order, handed out topmost first.
pub fn allocate(costs: &[Cost]) -> Vec<Grant> {
    allocate_within(costs, GLYPHS_MAX, PATH_BYTES_MAX)
}

fn allocate_within(costs: &[Cost], glyphs_max: usize, path_bytes_max: u64) -> Vec<Grant> {
    // Walk from the top down: each item takes the richest detail its path bytes still fit,
    // dots being free, then its whole label if the glyph pool still holds it.
    let (mut glyphs, mut bytes) = (glyphs_max, path_bytes_max);
    let mut grants: Vec<Grant> = costs
        .iter()
        .rev()
        .map(|c| {
            let steps = match c.natural {
                Detail::Full => [
                    Some((Detail::Full, c.full)),
                    Some((Detail::Simple, c.simple)),
                ],
                Detail::Simple => [Some((Detail::Simple, c.simple)), None],
                Detail::Dot | _ => [None, None],
            };
            let (detail, cost) = steps
                .into_iter()
                .flatten()
                .find(|&(_, cost)| cost <= bytes)
                .unwrap_or((Detail::Dot, 0));
            bytes -= cost;
            let label = detail != Detail::Dot && c.glyphs > 0 && c.glyphs <= glyphs;
            if label {
                glyphs -= c.glyphs;
            }
            Grant { detail, label }
        })
        .collect();
    grants.reverse();
    grants
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use gpui_kit::{MonochromeSprite, PathCurve, PathPaint, PathTile, Quad};
    use placemat_core::scene::LabelLine;

    use super::*;

    #[test]
    fn gpui_instances_have_the_sizes_the_costs_assume() {
        assert_eq!(size_of::<MonochromeSprite>(), 112, "glyph sprite");
        assert_eq!(size_of::<PathCurve>(), 40, "path curve");
        assert_eq!(size_of::<PathTile>(), 56, "path tile");
        assert_eq!(size_of::<PathPaint>(), 112, "path paint");
        assert_eq!(size_of::<Quad>(), 160, "quad");
    }

    #[test]
    fn labels_go_to_the_topmost_items_whole() {
        let costs = [Cost::new(Detail::Full, 0, 0, 100); 3];
        let labels: Vec<bool> = allocate_within(&costs, 250, u64::MAX)
            .iter()
            .map(|g| g.label)
            .collect();
        assert_eq!(
            labels,
            [false, true, true],
            "topmost wins, never a split label"
        );
    }

    #[test]
    fn detail_steps_down_from_the_top_as_the_path_pool_runs_out() {
        let c = Cost::new(Detail::Full, 1000, 100, 0);
        let details: Vec<Detail> = allocate_within(&[c; 3], 0, c.full + c.simple)
            .iter()
            .map(|g| g.detail)
            .collect();
        assert_eq!(
            details,
            [Detail::Dot, Detail::Simple, Detail::Full],
            "{c:?}"
        );
    }

    #[test]
    fn a_label_too_big_for_the_pool_leaves_it_to_the_next() {
        let costs = [
            Cost::new(Detail::Full, 0, 0, 10),
            Cost::new(Detail::Full, 0, 0, GLYPHS_MAX + 1),
        ];
        let labels: Vec<bool> = allocate(&costs).iter().map(|g| g.label).collect();
        assert_eq!(
            labels,
            [true, false],
            "the oversized label alone is dropped"
        );
    }

    #[test]
    fn a_dot_never_gets_a_label() {
        let costs = [Cost::new(Detail::Dot, 0, 0, 10)];
        assert!(!allocate(&costs)[0].label, "a dot has no room for text");
    }

    #[test]
    fn label_cost_counts_non_whitespace_chars_and_one_quad_per_tag() {
        let lines = [LabelLine::new("a b"), LabelLine::new(" \tcé ")];
        assert_eq!(label_cost(&lines), 4, "{lines:?}");
        let tagged = [LabelLine::new("a\u{202E}")];
        assert_eq!(label_cost(&tagged), 1 + 8 + QUAD_GLYPHS, "{tagged:?}");
    }

    #[test]
    fn a_tag_quad_fits_in_the_glyph_slots_it_is_charged() {
        assert!(
            QUAD_GLYPHS * size_of::<MonochromeSprite>() >= size_of::<Quad>(),
            "a quad is charged at least its bytes"
        );
    }

    #[test]
    fn a_short_cubic_is_one_quadratic_split_three_ways() {
        assert_eq!(cubic_curves(0.0), 3, "a point still yields one quadratic");
        assert_eq!(
            cubic_curves(2.0),
            6,
            "under the tolerance: ceil of a fraction is 1"
        );
    }

    #[test]
    fn a_long_cubic_grows_by_the_cube_root() {
        // Twice 20.8 over sqrt(432) / 4 is just over 8, so its cube root rounds up to 3.
        assert_eq!(cubic_curves(8.0 * 2.6), 12, "cube root of the length");
        assert_eq!(cubic_curves(f32::INFINITY), u64::MAX, "saturates");
    }

    #[test]
    fn a_solid_stroke_costs_five_curves_a_segment_and_three_a_subpath() {
        assert_eq!(stroke_curves(4, 1, 0.0), 5 * 4 + 3, "one open subpath");
        assert_eq!(stroke_curves(4, 2, 0.0), 5 * 4 + 3 * 2, "two subpaths");
    }

    #[test]
    fn a_stroke_outline_crosses_both_sides_and_its_width_at_every_join_and_cap() {
        assert_eq!(
            stroke_l1(100.0, 10.0, 2, 1, 0.0),
            200.0 + 10.0 * (6.0 + 3.0),
            "solid"
        );
        // 1.5 periods start 2 dashes plus 1 partial: 3 more joins and 3 more subpaths.
        assert_eq!(
            stroke_l1(100.0, 10.0, 2, 1, 1.5),
            200.0 + 10.0 * (15.0 + 12.0),
            "dashed"
        );
    }

    #[test]
    fn every_dash_is_a_subpath_that_may_split_a_segment() {
        // 2.5 periods may start 3 dashes, plus one partial dash per subpath.
        assert_eq!(
            stroke_curves(1, 1, 2.5),
            5 * (1 + 4) + 3 * (1 + 4),
            "dashed"
        );
        assert_eq!(stroke_curves(1, 1, f32::NAN), u64::MAX, "fails closed");
    }

    #[test]
    fn cell_min_doubles_until_the_grid_has_at_most_4096_cells() {
        assert_eq!(
            cell_min(100.0, 100.0),
            16.0,
            "a small path keeps 16 px cells"
        );
        assert_eq!(
            cell_min(4096.0, 4096.0),
            128.0,
            "64 px cells make 65 x 65 > 4096"
        );
        assert_eq!(cell_min(1e6, 0.0), 256.0, "one row of 3907 cells");
    }

    #[test]
    fn path_cost_counts_listings_unclipped_and_tiles_inside_the_clip() {
        let clip = Rect::new(0.0, 0.0, 160.0, 160.0);
        let inside = Rect::new(0.0, 0.0, 160.0, 160.0);
        // 16 px cells over 160 px: 11 x 11 cells, all visible; L1 of 320 crosses 20 lines.
        assert_eq!(
            path_cost(4, inside, 320.0, clip),
            112 + 40 * (12 + 20) + 56 * 121,
            "inside"
        );
        let off = Rect::new(1000.0, 0.0, 1160.0, 160.0);
        assert_eq!(
            path_cost(4, off, 320.0, clip),
            112 + 40 * (12 + 20),
            "no visible tile"
        );
    }

    #[test]
    fn tiles_never_exceed_the_grid_cap() {
        let huge = Rect::new(-1e7, -1e7, 1e7, 1e7);
        let clip = Rect::new(0.0, 0.0, 2560.0, 1600.0);
        let cost = path_cost(0, huge, 0.0, clip);
        assert!(cost <= 112 + 56 * 4096, "{cost}");
    }

    #[test]
    fn a_non_finite_path_prices_at_the_maximum() {
        let clip = Rect::new(0.0, 0.0, 10.0, 10.0);
        let endless = Rect::new(f64::NEG_INFINITY, 0.0, 1.0, 1.0);
        assert_eq!(
            path_cost(1, endless, 1.0, clip),
            u64::MAX,
            "infinite bounds"
        );
        let unit = Rect::new(0.0, 0.0, 1.0, 1.0);
        assert_eq!(path_cost(1, unit, f64::NAN, clip), u64::MAX, "NaN length");
        assert_eq!(
            path_cost(1, unit, f64::INFINITY, clip),
            u64::MAX,
            "infinite length"
        );
    }

    #[test]
    fn a_dot_with_a_label_is_whole_without_it() {
        let cost = Cost::new(Detail::Dot, 0, 0, 10);
        assert!(
            whole(&cost, &Grant::new(Detail::Dot, false)),
            "a dot never paints a label"
        );
    }
}
