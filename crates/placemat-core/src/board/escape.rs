//! The one escape table for quoted board strings. The writer quotes and the limits count from
//! the same [`escapes`] walk, so a string's counted size is the size it is written at.

use crate::text::{Piece, pieces};

/// How one character is written inside a quoted string.
pub(crate) enum Escape {
    /// A backslash and this character, such as `\n`.
    Short(char),
    /// `\u{hex}` with this code point in lowercase, unpadded hex.
    Unicode(u32),
}

/// The escape for `c`, or `None` when it is written as itself. Quotes, backslashes, controls
/// and the bidi and line-breaking invisibles are escaped so a review diff shows them.
pub(crate) fn escape(c: char) -> Option<Escape> {
    let short = match c {
        '\\' => '\\',
        '"' => '"',
        '\n' => 'n',
        '\r' => 'r',
        '\t' => 't',
        '\u{8}' => 'b',
        '\u{c}' => 'f',
        '\u{0}'..='\u{1f}'
        | '\u{7f}'..='\u{9f}'
        | '\u{200e}'..='\u{200f}'
        | '\u{2028}'..='\u{2029}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}'
        | '\u{feff}' => return Some(Escape::Unicode(u32::from(c))),
        _ => return None,
    };
    Some(Escape::Short(short))
}

/// Each character of `s` with its escape, or `None` when it is written as itself. Beyond the
/// always-escaped set of [`escape`], every code point [`pieces`] calls hidden in its place is
/// escaped, so a review diff shows it; context-kept selectors and flag tags stay raw.
pub(crate) fn escapes(s: &str) -> impl Iterator<Item = (char, Option<Escape>)> + '_ {
    pieces(s).flat_map(|piece| {
        let (text, hidden) = match piece {
            Piece::Text(t) => (t, None),
            Piece::Hidden(c) => ("", Some(c)),
        };
        let hidden = hidden.map(|c| (c, Some(escape(c).unwrap_or(Escape::Unicode(u32::from(c))))));
        text.chars().map(|c| (c, escape(c))).chain(hidden)
    })
}

/// The byte length of `s` once escaped, without the surrounding quotes.
pub(crate) fn escaped_len(s: &str) -> usize {
    escapes(s)
        .map(|(c, e)| match e {
            Some(Escape::Short(e)) => 1 + e.len_utf8(),
            // `\u{` and `}` around one hex digit per started nibble, at least one.
            Some(Escape::Unicode(cp)) => {
                4 + (u32::BITS - cp.leading_zeros()).div_ceil(4).max(1) as usize
            }
            None => c.len_utf8(),
        })
        .sum()
}

/// Appends `s` to `out` in double quotes, escaped by [`escapes`].
pub(crate) fn quote_into(s: &str, out: &mut String) {
    out.push('"');
    for (c, e) in escapes(s) {
        match e {
            Some(Escape::Short(e)) => {
                out.push('\\');
                out.push(e);
            }
            Some(Escape::Unicode(cp)) => out.push_str(&format!("\\u{{{cp:x}}}")),
            None => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::{escaped_len, quote_into};
    use crate::kdl::{ReadLimits, Value, parse};
    use crate::text::{Piece, pieces};

    /// `s` as the writer quotes it, without the surrounding quotes.
    fn body(s: &str) -> String {
        let mut out = String::new();
        quote_into(s, &mut out);
        out[1..out.len() - 1].to_owned()
    }

    /// The code points KDL v2 bans as literals anywhere in a document.
    fn kdl_banned(c: char) -> bool {
        matches!(c, '\u{0}'..='\u{8}' | '\u{e}'..='\u{1f}' | '\u{7f}' | '\u{200e}' | '\u{200f}'
            | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
    }

    #[test]
    fn hidden_characters_are_escaped() {
        for (s, want) in [
            ("\u{200b}", r"\u{200b}"),
            ("\u{2060}", r"\u{2060}"),
            ("\u{2064}", r"\u{2064}"),
            ("\u{180e}", r"\u{180e}"),
            ("\u{e0001}", r"\u{e0001}"),
            ("\u{e0041}", r"\u{e0041}"),
            ("\u{200e}", r"\u{200e}"),
            ("\u{200f}", r"\u{200f}"),
        ] {
            assert_eq!(body(s), want, "{s:?}");
        }
    }

    #[test]
    fn hidden_controls_keep_their_short_escapes() {
        assert_eq!(body("\r\u{8}\u{c}"), r"\r\b\f");
    }

    #[test]
    fn a_well_formed_subdivision_flag_is_written_raw() {
        let england = "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}";
        assert_eq!(body(england), england);
    }

    #[test]
    fn malformed_flags_escape_their_tags_but_not_the_base() {
        for (s, want) in [
            (
                "\u{1f3f4}\u{e0047}\u{e0042}\u{e0045}\u{e004e}\u{e0047}\u{e007f}",
                "\u{1f3f4}\\u{e0047}\\u{e0042}\\u{e0045}\\u{e004e}\\u{e0047}\\u{e007f}",
            ),
            (
                "\u{1f3f4}\u{e0061}\u{e0062}\u{e0063}\u{e0064}\u{e0065}\u{e0066}\u{e0067}\u{e0068}\u{e007f}",
                "\u{1f3f4}\\u{e0061}\\u{e0062}\\u{e0063}\\u{e0064}\\u{e0065}\\u{e0066}\\u{e0067}\\u{e0068}\\u{e007f}",
            ),
            (
                "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}",
                "\u{1f3f4}\\u{e0067}\\u{e0062}\\u{e0065}",
            ),
            ("x\u{e007f}", "x\\u{e007f}"),
        ] {
            assert_eq!(body(s), want, "{s:?}");
        }
    }

    #[test]
    fn selectors_are_raw_only_where_attached() {
        for (s, want) in [
            ("\u{2764}\u{fe0f}", "\u{2764}\u{fe0f}"),
            ("\u{2764}\u{fe0f}\u{fe0f}", "\u{2764}\u{fe0f}\\u{fe0f}"),
            ("\u{fe0f}", "\\u{fe0f}"),
            ("\n\u{fe0f}", "\\n\\u{fe0f}"),
            ("\u{200b}\u{fe0f}", "\\u{200b}\\u{fe0f}"),
            ("\u{200e}\u{fe0f}", "\\u{200e}\u{fe0f}"),
            ("葛\u{e0100}", "葛\u{e0100}"),
            ("a\u{e0100}", "a\\u{e0100}"),
        ] {
            assert_eq!(body(s), want, "{s:?}");
        }
    }

    #[test]
    fn joiners_and_the_arabic_letter_mark_stay_raw() {
        for s in [
            "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
            "می\u{200c}خواهم",
            "\u{61c}",
        ] {
            assert_eq!(body(s), s);
        }
    }

    // Every scalar value, alone (a newline keeps nothing, as the start does), after `a` and
    // after an ideograph, must be written so it reads back exactly, at its counted length,
    // with nothing hidden and nothing KDL bans left in the written text. Chunked for speed.
    #[test]
    fn every_scalar_round_trips_through_the_reader() {
        let scalars: Vec<char> = (0..=0x10_ffff).filter_map(char::from_u32).collect();
        for context in ["", "a", "葛"] {
            for chunk in scalars.chunks(8192) {
                let s: String = chunk.iter().map(|c| format!("{context}{c}\n")).collect();
                let written = body(&s);
                assert_eq!(escaped_len(&s), written.len(), "length after {context:?}");
                assert!(
                    pieces(&written).all(|p| matches!(p, Piece::Text(_))),
                    "hidden left after {context:?}"
                );
                assert!(!written.chars().any(kdl_banned), "banned after {context:?}");
                let nodes = parse(&format!("n \"{written}\""), &ReadLimits::default()).unwrap();
                assert_eq!(nodes[0].entries[0].value, Value::String(s));
            }
        }
    }

    #[test]
    fn quoting_writes_exactly_the_escaped_length() {
        for s in [
            "a",
            "é",
            "\"",
            "\n",
            "\u{0}",
            "\u{1f}",
            "\u{9f}",
            "\u{202e}",
            "\u{feff}",
            "\u{10ffff}",
        ] {
            let mut out = String::new();
            quote_into(s, &mut out);
            assert_eq!(out.len(), escaped_len(s) + 2, "{s:?} as {out}");
        }
    }

    #[test]
    fn escaped_lengths_match_the_written_forms() {
        for (s, len) in [
            ("a", 1),
            ("é", 2),
            ("\"", 2),
            ("\n", 2),
            ("\u{0}", 5),
            ("\u{1f}", 6),
            ("\u{9f}", 6),
            ("\u{202e}", 8),
            ("\u{feff}", 8),
        ] {
            assert_eq!(escaped_len(s), len, "{s:?}");
        }
    }
}
