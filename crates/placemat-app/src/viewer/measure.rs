//! Text measurement for scene layout, backed by GPUI's text system so labels lay out at the
//! widths they paint at.

use std::sync::Arc;

use gpui_kit::{FontId, TextSystem, font, px};
use placemat_core::scene::TextMeasure;

/// Line height as a multiple of the font size, for layout and for painting labels.
pub const LINE_HEIGHT_EM: f32 = 1.25;

/// The fraction of an em a char counts for when the text system gives it no advance.
const MISSING_ADVANCE_EM: f32 = 0.5;

/// Measures lines in Inter through GPUI's text system.
pub struct GpuiMeasure {
    text: Arc<TextSystem>,
    font: FontId,
}

impl GpuiMeasure {
    /// A measurer over `text`, resolving Inter once.
    pub fn new(text: Arc<TextSystem>) -> Self {
        let font = text.resolve_font(&font("Inter"));
        Self { text, font }
    }
}

impl TextMeasure for GpuiMeasure {
    fn line_width(&self, line: &str, size: f32) -> f32 {
        // Advances scale linearly with size, so world units in give world units out.
        advance_sum(line, size, |c| {
            self.text
                .advance(self.font, px(size), c)
                .ok()
                .map(|s| s.width.into())
        })
    }

    fn line_height(&self, size: f32) -> f32 {
        LINE_HEIGHT_EM * size
    }
}

/// Sum of per-char advances at `size`; a char whose advance is None counts 0.5 * size.
pub fn advance_sum(line: &str, size: f32, advance: impl Fn(char) -> Option<f32>) -> f32 {
    line.chars()
        .map(|c| advance(c).unwrap_or(MISSING_ADVANCE_EM * size))
        .sum()
}
