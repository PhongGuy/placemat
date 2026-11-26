//! The per-frame instance budget: how much detail and which labels a frame can afford before
//! GPUI's instance buffer overflows.

use placemat_core::scene::Detail;

/// The most label glyphs one frame paints: 7 MiB of 112-byte sprites.
pub const GLYPHS_MAX: usize = 65_536;
/// The estimated path bytes one frame paints: half of GPUI's 128 MiB instance cap, so paths,
/// glyphs, dots and chrome together still leave headroom for the estimate's tile count.
pub const PATH_BYTES_MAX: u64 = 64 << 20;
/// The most curves one op of an op set becomes once stroked.
pub const CURVES_PER_OP: u64 = 8;
/// The most curves one dash of a dashed stroke adds.
pub const CURVES_PER_DASH: u64 = 6;

/// The non-whitespace chars of a label: one glyph sprite each.
pub fn label_cost(lines: &[String]) -> usize {
    lines
        .iter()
        .flat_map(|l| l.chars())
        .filter(|c| !c.is_whitespace())
        .count()
}

/// The estimated instance bytes of one path of `ops` ops, `dashes` dashes and `len_device_px`
/// device pixels of length.
pub fn path_cost(ops: usize, dashes: f32, len_device_px: f32) -> u64 {
    // A curve costs its own 40 bytes plus about one 56-byte tile; a long path also crosses a
    // tile every 16 device pixels. Float casts saturate, so a huge path costs u64::MAX.
    let curves = (CURVES_PER_OP as f64) * ops as f64
        + (CURVES_PER_DASH as f64) * f64::from(dashes)
        + f64::from(len_device_px) / 16.0;
    (curves.ceil() as u64)
        .saturating_mul(96)
        .saturating_add(112)
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
    fn label_cost_counts_non_whitespace_chars() {
        let lines = ["a b".to_owned(), " \tcé ".to_owned()];
        assert_eq!(label_cost(&lines), 4, "{lines:?}");
    }

    #[test]
    fn path_cost_charges_a_paint_and_a_curve_and_tile_per_curve() {
        assert_eq!(path_cost(0, 0.0, 0.0), 112, "an empty path still paints");
        assert_eq!(path_cost(1, 0.0, 0.0), 112 + 96 * CURVES_PER_OP, "one op");
        assert_eq!(
            path_cost(0, 1.0, 32.0),
            112 + 96 * (CURVES_PER_DASH + 2),
            "a dash"
        );
        assert_eq!(path_cost(0, 0.0, f32::INFINITY), u64::MAX, "saturates");
    }
}
