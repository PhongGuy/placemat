//! The one escape table for quoted board strings. The writer quotes with [`escape`] and the
//! limits count with [`escaped_len`], so a string's counted size is the size it is written at.

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

/// The byte length of `s` once escaped, without the surrounding quotes.
pub(crate) fn escaped_len(s: &str) -> usize {
    s.chars()
        .map(|c| match escape(c) {
            Some(Escape::Short(e)) => 1 + e.len_utf8(),
            // `\u{` and `}` around one hex digit per started nibble, at least one.
            Some(Escape::Unicode(cp)) => {
                4 + (u32::BITS - cp.leading_zeros()).div_ceil(4).max(1) as usize
            }
            None => c.len_utf8(),
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::escaped_len;

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
