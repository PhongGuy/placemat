//! Text layout for free text and box and arrow labels: lines capped in number and length, and
//! the block measured through the injected [`TextMeasure`], sanitised.

use super::{LabelLine, TextMeasure};
use crate::text::{self, Piece};

/// The most lines a block keeps; later lines are dropped.
pub(crate) const LINES_MAX: usize = 16;
/// The most characters a line keeps; the rest is cut.
pub(crate) const CHARS_MAX: usize = 256;
/// The world font size of box and arrow labels.
pub(crate) const BOX_LABEL_SIZE: f32 = 20.0;

/// The first 16 lines (split on '\n', a trailing '\r' dropped), each cut to 256 source chars
/// and then expanded for display, and the block's width (widest display line) and height
/// (lines * line height); a non-finite or negative measurement counts as 0.
pub(crate) fn layout(text: &str, size: f32, m: &dyn TextMeasure) -> (Vec<LabelLine>, f32, f32) {
    let lines: Vec<LabelLine> = text
        .split('\n')
        .take(LINES_MAX)
        .map(|l| {
            let l = l.strip_suffix('\r').unwrap_or(l);
            let capped: String = l.chars().take(CHARS_MAX).collect();
            LabelLine::new(&capped)
        })
        .collect();
    let w = lines
        .iter()
        .map(|l| sane(m.line_width(&l.text, size)))
        .fold(0.0, f32::max);
    let h = sane(sane(m.line_height(size)) * lines.len() as f32);
    (lines, w, h)
}

impl LabelLine {
    /// One source line expanded for display, with no cap: each hidden code point becomes its
    /// tag, recorded as a byte range, and each tab one space.
    pub fn new(line: &str) -> LabelLine {
        let mut out = String::with_capacity(line.len());
        let mut hidden = Vec::new();
        for piece in text::pieces(line) {
            match piece {
                Piece::Text(t) => out.extend(t.chars().map(|c| if c == '\t' { ' ' } else { c })),
                Piece::Hidden(c) => {
                    let start = out.len();
                    out.push_str(&text::tag(c));
                    hidden.push(start..out.len());
                }
            }
        }
        LabelLine { text: out, hidden }
    }
}

/// A measurement the layout can trust: finite and non-negative, else 0.
fn sane(v: f32) -> f32 {
    if v.is_finite() && v > 0.0 { v } else { 0.0 }
}

#[cfg(test)]
mod tests;
