//! Text blocks: the line and character caps, CRLF, and a measurer that misbehaves.

use super::{CHARS_MAX, LINES_MAX, layout};
use crate::scene::{FixedAdvance, TextMeasure};

const STUB: FixedAdvance = FixedAdvance::new(0.5, 1.25);

#[test]
fn the_block_is_the_widest_of_the_first_16_lines() {
    let text = (1..=20)
        .map(|n| "x".repeat(n))
        .collect::<Vec<_>>()
        .join("\n");
    let (lines, w, h) = layout(&text, 20.0, &STUB);
    assert_eq!(lines.len(), LINES_MAX);
    assert_eq!(lines.last().map(String::len), Some(16));
    assert_eq!((w, h), (16.0 * 0.5 * 20.0, 16.0 * 1.25 * 20.0));
}

#[test]
fn long_lines_are_cut_at_256_characters_and_crlf_is_one_break() {
    let (lines, w, _) = layout(&format!("{}\r\nb", "é".repeat(300)), 10.0, &STUB);
    let counts: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
    assert_eq!(counts, [CHARS_MAX, 1]);
    assert_eq!(w, 256.0 * 0.5 * 10.0);
}

struct Hostile(f32);

impl TextMeasure for Hostile {
    fn line_width(&self, _: &str, _: f32) -> f32 {
        self.0
    }
    fn line_height(&self, _: f32) -> f32 {
        self.0
    }
}

#[test]
fn a_hostile_measurer_gives_an_empty_block() {
    for v in [f32::NAN, f32::INFINITY, -1.0] {
        let (lines, w, h) = layout("abc\ndef", 20.0, &Hostile(v));
        assert_eq!((lines.len(), w, h), (2, 0.0, 0.0), "{v}");
    }
}
