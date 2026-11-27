//! Text blocks: the line and character caps, CRLF, and a measurer that misbehaves.

use super::{CHARS_MAX, LINES_MAX, layout};
use crate::scene::{FixedAdvance, LabelLine, TextMeasure};

const STUB: FixedAdvance = FixedAdvance::new(0.5, 1.25);

#[test]
fn the_block_is_the_widest_of_the_first_16_lines() {
    let text = (1..=20)
        .map(|n| "x".repeat(n))
        .collect::<Vec<_>>()
        .join("\n");
    let (lines, w, h) = layout(&text, 20.0, &STUB);
    assert_eq!(lines.len(), LINES_MAX);
    assert_eq!(lines.last().map(|l| l.text.len()), Some(16));
    assert_eq!((w, h), (16.0 * 0.5 * 20.0, 16.0 * 1.25 * 20.0));
}

#[test]
fn long_lines_are_cut_at_256_characters_and_crlf_is_one_break() {
    let (lines, w, _) = layout(&format!("{}\r\nb", "é".repeat(300)), 10.0, &STUB);
    let counts: Vec<usize> = lines.iter().map(|l| l.text.chars().count()).collect();
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

const ENGLAND: &str = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";

fn texts(lines: &[LabelLine]) -> Vec<&str> {
    lines.iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn the_cap_counts_source_chars_and_expansion_follows() {
    let (lines, _, _) = layout(&"\u{202E}".repeat(300), 20.0, &STUB);
    let [line] = lines.as_slice() else {
        panic!("{lines:?}")
    };
    assert_eq!(line.text, "<U+202E>".repeat(CHARS_MAX));
    assert_eq!(line.hidden.len(), CHARS_MAX);
}

#[test]
fn the_hidden_ranges_slice_to_exactly_the_tags_and_the_block_measures_them() {
    let (lines, w, _) = layout("a\u{202E}b\u{200B}\r\n\u{7}", 10.0, &STUB);
    assert_eq!(texts(&lines), ["a<U+202E>b<U+200B>", "<U+0007>"]);
    let tags: Vec<Vec<&str>> = lines
        .iter()
        .map(|l| {
            l.hidden
                .iter()
                .map(|r| l.text.get(r.clone()).unwrap())
                .collect()
        })
        .collect();
    assert_eq!(tags, [vec!["<U+202E>", "<U+200B>"], vec!["<U+0007>"]]);
    assert_eq!(w, 18.0 * 0.5 * 10.0, "the block measures the display text");
}

#[test]
fn a_mid_line_cr_tags_a_trailing_one_breaks_and_a_tab_is_a_space() {
    let (lines, _, _) = layout("a\rb\r\nc\td", 10.0, &STUB);
    assert_eq!(texts(&lines), ["a<U+000D>b", "c d"]);
    assert_eq!(
        lines.iter().map(|l| l.hidden.len()).collect::<Vec<_>>(),
        [1, 0]
    );
}

#[test]
fn a_cap_inside_a_flag_tags_its_leftover_tag_characters() {
    let (lines, _, _) = layout(&format!("{}{ENGLAND}", "a".repeat(250)), 10.0, &STUB);
    let want = format!(
        "{}\u{1F3F4}<U+E0067><U+E0062><U+E0065><U+E006E><U+E0067>",
        "a".repeat(250)
    );
    assert_eq!(texts(&lines), [want.as_str()]);
}

#[test]
fn a_label_line_of_plain_text_has_no_hidden_ranges() {
    let line = LabelLine::new("plain");
    assert_eq!((line.text.as_str(), line.hidden.len()), ("plain", 0));
}
