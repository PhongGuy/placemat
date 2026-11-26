//! The hidden set: every table edge, the kept contexts, flags, and the round trip.

use super::{HIDDEN, Piece, UNIFIED_IDEOGRAPH, display, is_hidden, pieces, tag};

const ENGLAND: &str = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";

fn ch(c: u32) -> char {
    char::from_u32(c).unwrap()
}

fn in_ranges(t: &[(u32, u32)], c: u32) -> bool {
    t.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
}

#[test]
fn the_tables_match_the_ucd_16_note() {
    assert_eq!(HIDDEN.len(), 19);
    assert_eq!(
        HIDDEN.iter().map(|&(lo, hi)| hi - lo + 1).sum::<u32>(),
        4237
    );
    assert_eq!(UNIFIED_IDEOGRAPH.len(), 17);
    for t in [&HIDDEN[..], &UNIFIED_IDEOGRAPH[..]] {
        assert!(t.iter().all(|&(lo, hi)| lo <= hi), "{t:x?}");
        assert!(
            t.windows(2).all(|w| w[0].1 + 1 < w[1].0),
            "ascending, disjoint, merged: {t:x?}"
        );
    }
}

#[test]
fn a_tag_is_uppercase_hex_of_at_least_four_digits() {
    assert_eq!(tag('\u{7}'), "<U+0007>");
    assert_eq!(tag('\u{202E}'), "<U+202E>");
    assert_eq!(tag('\u{E0FFF}'), "<U+E0FFF>");
}

#[test]
fn each_hidden_range_edge_tags_and_its_neighbours_do_not() {
    for &(lo, hi) in &HIDDEN {
        for c in [lo, hi] {
            assert_eq!(display(&ch(c).to_string()), tag(ch(c)), "{c:04X}");
        }
        for n in [lo.checked_sub(1), hi.checked_add(1)].into_iter().flatten() {
            let Some(c) = char::from_u32(n) else { continue };
            if in_ranges(&HIDDEN, n) {
                continue;
            }
            let shown = display(&c.to_string());
            assert!(!shown.contains("<U+"), "{n:04X} gave {shown}");
        }
    }
}

#[test]
fn is_hidden_answers_false_for_kept_and_context_kept_code_points() {
    for c in [
        '\u{202E}',
        '\u{2066}',
        '\u{200B}',
        '\u{180E}',
        '\u{E0001}',
        '\r',
        '\u{0}',
    ] {
        assert!(is_hidden(c), "{c:?}");
    }
    for c in [
        'a', '\n', '\t', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}', '\u{061C}',
    ] {
        assert!(!is_hidden(c), "{c:?}");
    }
    for c in [
        '\u{FE0F}',
        '\u{034F}',
        '\u{180B}',
        '\u{E0067}',
        '\u{E007F}',
        '\u{E0100}',
    ] {
        assert!(!is_hidden(c), "context-kept {c:?}");
    }
}

#[test]
fn shaping_and_direction_marks_and_prepended_marks_pass_through() {
    for s in [
        "\u{645}\u{631}\u{62D}\u{628}\u{627}\u{200F} \u{628}\u{627}",
        "\u{645}\u{6CC}\u{200C}\u{62E}\u{648}\u{627}\u{647}\u{645}",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "a\u{061C}b\u{200E}c",
        "\u{0600}\u{0661}",
        "\u{13430}",
    ] {
        assert_eq!(display(s), s, "{s:?}");
    }
}

#[test]
fn an_attached_selector_is_kept_only_after_a_shown_code_point() {
    assert_eq!(display("\u{2764}\u{FE0F}"), "\u{2764}\u{FE0F}");
    assert_eq!(
        display("\u{2764}\u{FE0F}\u{FE0F}"),
        "\u{2764}\u{FE0F}<U+FE0F>"
    );
    assert_eq!(display("\u{1820}\u{180B}"), "\u{1820}\u{180B}");
    assert_eq!(display("a\u{034F}b"), "a\u{034F}b");
    assert_eq!(display("\u{FE0F}x"), "<U+FE0F>x");
    assert_eq!(display("\u{202E}\u{FE0F}"), "<U+202E><U+FE0F>");
    assert_eq!(display("a\n\u{FE0F}"), "a\n<U+FE0F>");
    assert_eq!(display("a\t\u{FE0F}"), "a <U+FE0F>");
}

#[test]
fn an_ideographic_selector_is_kept_only_after_a_unified_ideograph() {
    for &(lo, hi) in &UNIFIED_IDEOGRAPH {
        for c in [lo, hi] {
            let s = format!("{}\u{E0100}", ch(c));
            assert_eq!(display(&s), s, "{c:04X}");
        }
        for n in [lo - 1, hi + 1] {
            if in_ranges(&UNIFIED_IDEOGRAPH, n) {
                continue;
            }
            let c = ch(n);
            assert_eq!(
                display(&format!("{c}\u{E0100}")),
                format!("{c}<U+E0100>"),
                "{n:04X}"
            );
        }
    }
    assert_eq!(display("\u{845B}\u{E0100}"), "\u{845B}\u{E0100}");
    assert_eq!(display("a\u{E0100}"), "a<U+E0100>");
    assert_eq!(
        display("\u{845B}\u{E0100}\u{E0101}"),
        "\u{845B}\u{E0100}<U+E0101>"
    );
}

#[test]
fn a_well_formed_subdivision_flag_is_kept() {
    assert_eq!(display(ENGLAND), ENGLAND);
    assert_eq!(display(&format!("go {ENGLAND}!")), format!("go {ENGLAND}!"));
    assert_eq!(display(&ENGLAND.repeat(2)), ENGLAND.repeat(2));
    let digits = "\u{1F3F4}\u{E0066}\u{E0072}\u{E0037}\u{E0035}\u{E007F}";
    assert_eq!(display(digits), digits);
    let seven = format!("\u{1F3F4}{}\u{E007F}", "\u{E0061}".repeat(7));
    assert_eq!(display(&seven), seven);
    assert_eq!(display("\u{1F3F4}"), "\u{1F3F4}");
}

#[test]
fn stray_or_malformed_tag_characters_are_tagged() {
    assert_eq!(display("a\u{E0067}b"), "a<U+E0067>b");
    assert_eq!(display("\u{E007F}"), "<U+E007F>");
    assert_eq!(
        display("\u{1F3F4}\u{E0047}\u{E0042}\u{E0045}\u{E007F}"),
        "\u{1F3F4}<U+E0047><U+E0042><U+E0045><U+E007F>"
    );
    let eight = format!("\u{1F3F4}{}\u{E007F}", "\u{E0061}".repeat(8));
    assert_eq!(
        display(&eight),
        format!("\u{1F3F4}{}<U+E007F>", "<U+E0061>".repeat(8))
    );
    assert_eq!(
        display("\u{1F3F4}\u{E0061}\u{E0062}\u{E007F}"),
        "\u{1F3F4}<U+E0061><U+E0062><U+E007F>"
    );
    assert_eq!(
        display("\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}"),
        "\u{1F3F4}<U+E0067><U+E0062><U+E0065>"
    );
}

#[test]
fn tab_is_a_space_newline_passes_and_other_controls_tag() {
    assert_eq!(display("a\tb"), "a b");
    assert_eq!(display("a\nb"), "a\nb");
    assert_eq!(display("a\rb"), "a<U+000D>b");
    assert_eq!(display("report\u{202E}txt.exe"), "report<U+202E>txt.exe");
    assert_eq!(
        display("\u{0}\u{7}\u{85}\u{2028}\u{FEFF}\u{E0FFF}"),
        "<U+0000><U+0007><U+0085><U+2028><U+FEFF><U+E0FFF>"
    );
}

#[test]
fn deprecated_and_blank_code_points_are_tagged_on_purpose() {
    for c in [
        '\u{AD}',
        '\u{115F}',
        '\u{1160}',
        '\u{3164}',
        '\u{FFA0}',
        '\u{17B4}',
        '\u{206A}',
        '\u{1BCA0}',
        '\u{1D173}',
        '\u{FFF9}',
        '\u{FFFB}',
    ] {
        assert_eq!(display(&c.to_string()), tag(c), "{c:?}");
    }
}

#[test]
fn pieces_reproduce_the_input_and_split_out_each_hidden_code_point() {
    let s = format!("a\t\u{202E}\u{202E}b{ENGLAND}\u{FE0F}\u{FE0F}\r");
    let got: Vec<Piece<'_>> = pieces(&s).collect();
    let text = format!("b{ENGLAND}\u{FE0F}");
    assert_eq!(
        got,
        [
            Piece::Text("a\t"),
            Piece::Hidden('\u{202E}'),
            Piece::Hidden('\u{202E}'),
            Piece::Text(&text),
            Piece::Hidden('\u{FE0F}'),
            Piece::Hidden('\r'),
        ]
    );
    let back: String = got
        .iter()
        .map(|p| match p {
            Piece::Text(t) => (*t).to_owned(),
            Piece::Hidden(c) => c.to_string(),
        })
        .collect();
    assert_eq!(back, s);
    assert_eq!(pieces("").count(), 0);
}
