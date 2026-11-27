//! Hidden text: code points that render as nothing or reorder the text around them, shown as
//! visible `<U+XXXX>` tags at display time while the source string stays untouched. The
//! tables are UCD 16.0's and change only with a new Unicode version.

/// UCD 16.0 hidden rule minus the always-kept U+061C and U+200C-200F.
const HIDDEN: [(u32, u32); 19] = [
    (0x0000, 0x0008),
    (0x000B, 0x001F),
    (0x007F, 0x009F),
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x200B, 0x200B),
    (0x2028, 0x202E),
    (0x2060, 0x206F),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0F),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF0, 0xFFFB),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE0FFF),
];
/// UCD 16.0 Unified_Ideograph (PropList.txt).
const UNIFIED_IDEOGRAPH: [(u32, u32); 17] = [
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xFA0E, 0xFA0F),
    (0xFA11, 0xFA11),
    (0xFA13, 0xFA14),
    (0xFA1F, 0xFA1F),
    (0xFA21, 0xFA21),
    (0xFA23, 0xFA24),
    (0xFA27, 0xFA29),
    (0x20000, 0x2A6DF),
    (0x2A700, 0x2B739),
    (0x2B740, 0x2B81D),
    (0x2B820, 0x2CEA1),
    (0x2CEB0, 0x2EBE0),
    (0x2EBF0, 0x2EE5D),
    (0x30000, 0x3134A),
    (0x31350, 0x323AF),
];

/// One stretch of a string, as [`pieces`] cuts it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    /// A maximal, non-empty run of shown text; it may hold tabs and newlines.
    Text(&'a str),
    /// One hidden code point, shown as its [`tag`].
    Hidden(char),
}

/// `s` cut into maximal runs of shown text and single hidden code points, in order.
pub fn pieces(s: &str) -> impl Iterator<Item = Piece<'_>> {
    Pieces {
        s,
        pos: 0,
        prev: None,
    }
}

/// Whether `c` is hidden wherever it stands; context-kept code points answer false.
pub fn is_hidden(c: char) -> bool {
    in_table(&HIDDEN, c) && !is_context(c)
}

/// The visible tag for `c`: `<U+202E>`, `<U+0007>`, `<U+E0FFF>`.
pub fn tag(c: char) -> String {
    format!("<U+{:04X}>", u32::from(c))
}

/// `s` with every hidden code point replaced by its tag and every tab by one space.
pub fn display(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for piece in pieces(s) {
        match piece {
            Piece::Text(t) => out.extend(t.chars().map(|c| if c == '\t' { ' ' } else { c })),
            Piece::Hidden(c) => out.push_str(&tag(c)),
        }
    }
    out
}

/// The black flag that opens a subdivision flag such as England's.
const BLACK_FLAG: char = '\u{1F3F4}';
/// The cancel tag that closes a subdivision flag.
const CANCEL_TAG: char = '\u{E007F}';

/// Whether `c` falls in one of the inclusive ranges of `table`.
fn in_table(table: &[(u32, u32)], c: char) -> bool {
    let c = u32::from(c);
    table.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
}

/// A selector that modifies the code point before it: variation selectors, Mongolian free
/// variation selectors and the combining grapheme joiner.
fn is_attached_selector(c: char) -> bool {
    matches!(c, '\u{034F}' | '\u{180B}'..='\u{180D}' | '\u{180F}' | '\u{FE00}'..='\u{FE0F}')
}

/// An ideographic variation selector, which only modifies a unified ideograph.
fn is_ideographic_selector(c: char) -> bool {
    matches!(c, '\u{E0100}'..='\u{E01EF}')
}

/// A hidden code point that is shown in the right context: a selector or a tag character.
fn is_context(c: char) -> bool {
    is_attached_selector(c) || is_ideographic_selector(c) || matches!(c, '\u{E0020}'..='\u{E007F}')
}

/// Whether context code point `c` is kept after `prev`, the code point passed through just
/// before it. Tag characters are kept only inside a whole flag, which [`flag_len`] finds.
fn kept_after(c: char, prev: Option<char>) -> bool {
    let Some(p) = prev else { return false };
    if is_attached_selector(c) {
        p != '\n' && p != '\t' && !is_attached_selector(p) && !is_ideographic_selector(p)
    } else {
        is_ideographic_selector(c) && in_table(&UNIFIED_IDEOGRAPH, p)
    }
}

/// The byte length of the tag sequence closing a subdivision flag at the start of `rest`
/// (3 to 7 tag digits or lowercase letters, then the cancel tag), if it is well formed.
fn flag_len(rest: &str) -> Option<usize> {
    let mut len = 0;
    for (n, c) in rest.chars().enumerate() {
        len += c.len_utf8();
        match c {
            CANCEL_TAG if (3..=7).contains(&n) => return Some(len),
            '\u{E0030}'..='\u{E0039}' | '\u{E0061}'..='\u{E007A}' if n < 7 => {}
            _ => return None,
        }
    }
    None
}

/// The lazy cutter behind [`pieces`]: a byte cursor plus the last code point passed through,
/// which decides whether a selector at the cursor is attached or stray.
struct Pieces<'a> {
    /// The string being cut.
    s: &'a str,
    /// The byte offset where the next piece starts.
    pos: usize,
    /// The code point passed through just before `pos`; `None` after a hidden one or at start.
    prev: Option<char>,
}

impl<'a> Iterator for Pieces<'a> {
    type Item = Piece<'a>;

    // Extend a text run code point by code point; a hidden code point ends the run, and is
    // emitted on its own once the run before it has been returned. A whole flag joins the run.
    fn next(&mut self) -> Option<Piece<'a>> {
        let start = self.pos;
        while let Some(c) = self.s.get(self.pos..).and_then(|r| r.chars().next()) {
            let after = self.pos + c.len_utf8();
            if c == BLACK_FLAG
                && let Some(flag) = self.s.get(after..).and_then(flag_len)
            {
                self.pos = after + flag;
                self.prev = Some(CANCEL_TAG);
                continue;
            }
            if is_hidden(c) || (is_context(c) && !kept_after(c, self.prev)) {
                if self.pos > start {
                    return self.s.get(start..self.pos).map(Piece::Text);
                }
                self.pos = after;
                self.prev = None;
                return Some(Piece::Hidden(c));
            }
            self.pos = after;
            self.prev = Some(c);
        }
        if self.pos > start {
            self.s.get(start..self.pos).map(Piece::Text)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
