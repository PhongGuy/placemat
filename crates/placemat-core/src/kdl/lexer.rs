// SPDX-License-Identifier: MIT OR Apache-2.0
// Forked from kdlite 0.1.1 `src/stream.rs`, Copyright (c) 2025 1e1001 and (c) 2025 Ferron,
// used under MIT; owned by Placemat since, no upstream sync.
//! A streaming KDL v2 lexer: a counter-driven state machine over borrowed text, with no recursion.
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::arithmetic_side_effects
)]

use std::borrow::Cow;

#[cfg(test)]
mod tests;

/// A parsed value. Numbers stay as their source text.
#[derive(Debug, PartialEq)]
pub(crate) enum Value<'text> {
    /// A string, borrowed when it needed no unescaping
    String(Cow<'text, str>),
    /// A number, as written
    Number(&'text str),
    /// `#true` or `#false`
    Bool(bool),
    /// `#null`
    Null,
}

/// A parsing error
/// `usize` arguments are byte positions in the source text
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub(crate) enum Error {
    /// A space character was expected
    ExpectedSpace(usize),
    /// A `)` was expected
    ExpectedCloseParen(usize),
    /// A single-line comment was expected
    ExpectedComment(usize),
    /// A mandatory newline was expected here
    ExpectedNewline(usize),
    /// A string or identifier was expected
    ExpectedString(usize),
    /// A value of some kind was expected
    ExpectedValue(usize),
    /// A `}` was placed where it shouldn't be
    UnexpectedCloseBracket(usize),
    /// A newline is not allowed here
    UnexpectedNewline(usize),
    /// A number is invalid
    InvalidNumber(usize),
    /// A keyword has an invalid name
    BadKeyword(usize),
    /// An identifier with an invalid name
    BadIdentifier(usize),
    /// An invalid escape at this position
    BadEscape(usize),
    /// The indentation for this line doesn't match the string pattern
    BadIndent(usize),
    /// Multiple children blocks are present for one node
    MultipleChildren(usize),
    /// The file ended too early; the position is always the text length
    UnexpectedEof(usize),
    /// An always-invalid character at this position
    BannedChar(char, usize),
    /// A nesting or hash counter ran past `usize` here
    Overflow(usize),
}

impl Error {
    /// The byte position in the source text this error points at.
    pub(crate) fn offset(&self) -> usize {
        match *self {
            Self::ExpectedSpace(at)
            | Self::ExpectedCloseParen(at)
            | Self::ExpectedComment(at)
            | Self::ExpectedNewline(at)
            | Self::ExpectedString(at)
            | Self::ExpectedValue(at)
            | Self::UnexpectedCloseBracket(at)
            | Self::UnexpectedNewline(at)
            | Self::InvalidNumber(at)
            | Self::BadKeyword(at)
            | Self::BadIdentifier(at)
            | Self::BadEscape(at)
            | Self::BadIndent(at)
            | Self::MultipleChildren(at)
            | Self::UnexpectedEof(at)
            | Self::BannedChar(_, at)
            | Self::Overflow(at) => at,
        }
    }
}

type PResult<T> = Result<T, Error>;

/// a parsing event
#[derive(Debug, PartialEq)]
pub(crate) enum Event<'text> {
    /// A new node
    Node {
        /// Optional node type hint
        ty: Option<Cow<'text, str>>,
        /// Node name
        name: Cow<'text, str>,
        /// Byte position of the name, after any type hint
        start: usize,
    },
    /// A node value or property
    Entry {
        /// The key, if it exists
        key: Option<Cow<'text, str>>,
        /// Optional value type hint
        ty: Option<Cow<'text, str>>,
        /// Value
        value: Value<'text>,
        /// Byte position of a property's key, or of an argument's value after its type hint
        start: usize,
    },
    /// Start of children list for the previous node,
    /// will only be emitted once per node
    Begin {
        /// Byte position of the `{`
        offset: usize,
    },
    /// End of children list
    End {
        /// Byte position of the `}`
        offset: usize,
    },
}

#[derive(Debug)]
enum InnerEvent<'text> {
    Node {
        sd: bool,
        ty: Option<Cow<'text, str>>,
        name: Cow<'text, str>,
        start: usize,
    },
    PropValue {
        sd: bool,
        ty: Option<Cow<'text, str>>,
        key: Option<Cow<'text, str>>,
        value: Value<'text>,
        start: usize,
    },
    Begin {
        sd: bool,
        offset: usize,
    },
    End {
        offset: usize,
    },
    Done,
}

enum ParserState {
    /// right after init
    BeginDocument,
    /// next item is a node or ending
    NextNode,
    /// while parsing a node
    NodeProps,
    /// only children left
    NodeChildren,
    /// die
    Done,
}

/// A value that's been parsed enough to differentiate it
enum SemiValue<'text> {
    /// `string` always
    String(Cow<'text, str>),
    /// `number` or invalid
    Number(&'text str),
    /// `keyword` or invalid
    Keyword(&'text str),
}

/// parsing position
#[repr(transparent)]
#[derive(Clone, Copy)]
struct Pos(usize);

impl Pos {
    // A saturated position lies past every text, so it reads as end of input.
    fn offset_bytes(self, n: usize) -> Self {
        Self(self.0.saturating_add(n))
    }
    fn offset_char(self, ch: char) -> Self {
        self.offset_bytes(ch.len_utf8())
    }
    fn offset_str(self, text: &str) -> Self {
        self.offset_bytes(text.len())
    }
}

struct Grammar<'text>(&'text str);

// in this impl: anything in `backticks` (except that)
// represents a kdl grammar item or expression
impl<'text> Grammar<'text> {
    // Out of range reads as end of input; every other slice goes through `slice`.
    fn tail(&self, at: Pos) -> &'text str {
        self.0.get(at.0..).unwrap_or("")
    }
    fn slice(&self, from: Pos, to: Pos) -> PResult<&'text str> {
        self.0.get(from.0..to.0).ok_or_else(|| self.eof())
    }
    fn eof(&self) -> Error {
        Error::UnexpectedEof(self.0.len())
    }
    // TODO: i realize now this could be written a lot better as a
    // "(&Self, Pos) -> (char, Pos)", kinda like every other parser
    // would require rewriting every single parse rule but could be nice
    fn top_char(&self, at: Pos) -> Option<char> {
        self.tail(at).chars().next()
    }
    /// `bom`
    fn bom(&self, at: Pos) -> Option<Pos> {
        (self.top_char(at) == Some('\u{FEFF}')).then(|| at.offset_char('\u{FEFF}'))
    }
    // `disallowed-literal-code-points`
    fn banned(ch: char) -> bool {
        // D800-DFFF are not allowed by rust char
        matches!(ch, '\u{0}'..='\u{8}' | '\u{E}'..='\u{1F}' | '\u{7F}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
    }
    // `identifier-char`
    fn ident(ch: char) -> bool {
        !(Self::banned(ch)
            || Self::space(ch)
            || Self::newline(ch)
            || matches!(
                ch,
                '\\' | '/' | '(' | ')' | '{' | '}' | ';' | '[' | ']' | '"' | '#' | '='
            ))
    }
    // `unicode-space`
    fn space(ch: char) -> bool {
        matches!(
            ch,
            '\u{9}' | '\u{20}' | '\u{A0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
        )
    }
    // `newline`
    fn newline(ch: char) -> bool {
        matches!(ch, '\u{A}'..='\u{D}' | '\u{85}' | '\u{2028}' | '\u{2029}')
    }
    fn number_like(text: &str) -> bool {
        let text = text.strip_prefix(['+', '-']).unwrap_or(text);
        let text = text.strip_prefix('.').unwrap_or(text);
        text.as_bytes().first().is_some_and(u8::is_ascii_digit)
    }
    /// `number`, checked for syntax only: the text stays as written, so no width limit applies.
    /// Only lowercase radix prefixes count; `0X10` is a malformed decimal.
    fn is_number(text: &str) -> bool {
        // `digit (digit | '_')*` for a radix's digit class
        fn digits(text: &str, digit: fn(&char) -> bool) -> bool {
            let mut chars = text.chars();
            chars.next().as_ref().is_some_and(digit) && chars.all(|ch| ch == '_' || digit(&ch))
        }
        fn decimal(ch: &char) -> bool {
            ch.is_ascii_digit()
        }
        let body = text.strip_prefix(['+', '-']).unwrap_or(text);
        if let Some(hex) = body.strip_prefix("0x") {
            return digits(hex, char::is_ascii_hexdigit);
        }
        if let Some(octal) = body.strip_prefix("0o") {
            return digits(octal, |ch| matches!(ch, '0'..='7'));
        }
        if let Some(binary) = body.strip_prefix("0b") {
            return digits(binary, |ch| matches!(ch, '0' | '1'));
        }
        // `integer ('.' integer)? (('e' | 'E') sign? integer)?`
        let (mantissa, exponent) = match body.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => (mantissa, Some(exponent)),
            None => (body, None),
        };
        let mantissa_ok = match mantissa.split_once('.') {
            Some((whole, fraction)) => digits(whole, decimal) && digits(fraction, decimal),
            None => digits(mantissa, decimal),
        };
        mantissa_ok
            && exponent.is_none_or(|exponent| {
                digits(
                    exponent.strip_prefix(['+', '-']).unwrap_or(exponent),
                    decimal,
                )
            })
    }
    /// `single-line-comment` after `//`
    /// = `^newline* (newline | eof)`
    fn single_line_comment(&self, mut at: Pos) -> PResult<Pos> {
        loop {
            match self.top_char(at) {
                Some(ch) if Self::banned(ch) => return Err(Error::BannedChar(ch, at.0)),
                Some(ch) if Self::newline(ch) => return Ok(at.offset_char(ch)),
                None => return Ok(at),
                Some(ch) => at = at.offset_char(ch),
            }
        }
    }
    // `multi-line-comment` after `/*`
    // = `(…) */`
    fn multi_line_comment(&self, mut at: Pos) -> PResult<Pos> {
        let mut nest = 0_usize;
        loop {
            if self.tail(at).starts_with("*/") {
                at = at.offset_str("*/");
                if let Some(next) = nest.checked_sub(1) {
                    nest = next;
                } else {
                    return Ok(at);
                }
            } else if self.tail(at).starts_with("/*") {
                at = at.offset_str("/*");
                nest = nest.checked_add(1).ok_or(Error::Overflow(at.0))?;
            } else {
                match self.top_char(at) {
                    Some(ch) if Self::banned(ch) => return Err(Error::BannedChar(ch, at.0)),
                    Some(ch) => at = at.offset_char(ch),
                    None => return Err(self.eof()),
                }
            }
        }
    }
    // `escline` after `\`
    // = `ws* (single-line-comment | newline | eof)`
    fn escline(&self, mut at: Pos) -> PResult<Pos> {
        // valid: `unicode-space` `multi-line-comment`
        loop {
            match self.top_char(at) {
                Some(ch) if Self::space(ch) => {
                    at = at.offset_char(ch);
                }
                Some('/') if self.top_char(at.offset_char('/')) == Some('*') => {
                    at = self.multi_line_comment(at.offset_str("/*"))?;
                }
                _ => break,
            }
        }
        // valid: `single-line-comment` `newline` `eof`
        match self.top_char(at) {
            Some('/') if self.top_char(at.offset_char('/')) == Some('/') => {
                self.single_line_comment(at.offset_str("//"))
            }
            Some(ch) if Self::newline(ch) => Ok(at.offset_char(ch)),
            None => Ok(at),
            _ => Err(Error::ExpectedComment(at.0)),
        }
    }
    /// `line-space*`
    fn line_space(&self, mut at: Pos) -> PResult<Pos> {
        // valid:
        //  `unicode-space` `newline` `escline`
        //  `single-line-comment` `multi-line-comment`
        loop {
            match self.top_char(at) {
                Some('\\') => at = self.escline(at.offset_char('\\'))?,
                Some('/') => match self.top_char(at.offset_char('/')) {
                    Some('/') => at = self.single_line_comment(at.offset_str("//"))?,
                    Some('*') => at = self.multi_line_comment(at.offset_str("/*"))?,
                    _ => break,
                },
                Some(ch) if Self::newline(ch) || Self::space(ch) => {
                    at = at.offset_char(ch);
                }
                _ => break,
            }
        }
        Ok(at)
    }
    // `node-space*` or `node-space+`
    fn node_space(&self, start: Pos, req: bool) -> PResult<Pos> {
        let mut at = start;
        // valid: `unicode-space` `escline` `multi-line-comment`
        loop {
            match self.top_char(at) {
                Some('\\') => {
                    at = self.escline(at.offset_char('\\'))?;
                }
                Some('/') if self.top_char(at.offset_char('/')) == Some('*') => {
                    at = self.multi_line_comment(at.offset_str("/*"))?;
                }
                Some(ch) if Self::space(ch) => {
                    at = at.offset_char(ch);
                }
                _ => break,
            }
        }
        if !req || at.0 != start.0 {
            Ok(at)
        } else {
            Err(Error::ExpectedSpace(at.0))
        }
    }
    /// `slashdash`
    fn slash_dash(&self, at: Pos) -> PResult<Option<Pos>> {
        self.tail(at)
            .starts_with("/-")
            .then(|| self.line_space(at.offset_str("/-")))
            .transpose()
    }
    /// `identifier-string`
    fn identifier_string(&self, at: Pos) -> PResult<(Pos, &'text str)> {
        let mut end = at;
        while let Some(ch) = self.top_char(end) {
            if !Self::ident(ch) {
                break;
            }
            end = end.offset_char(ch);
        }
        Ok((end, self.slice(at, end)?))
    }
    /// string escape after \
    fn escape(&self, at: Pos) -> PResult<(Pos, Option<char>)> {
        let (at, ch) = match self.top_char(at).ok_or_else(|| self.eof())? {
            'n' => (at.offset_char('n'), Some('\n')),
            'r' => (at.offset_char('r'), Some('\r')),
            't' => (at.offset_char('t'), Some('\t')),
            '\\' => (at.offset_char('\\'), Some('\\')),
            '"' => (at.offset_char('"'), Some('"')),
            'b' => (at.offset_char('b'), Some('\x08')),
            'f' => (at.offset_char('f'), Some('\x0C')),
            's' => (at.offset_char('s'), Some(' ')),
            'u' => {
                let start = at.offset_char('u');
                let Some('{') = self.top_char(start) else {
                    return Err(Error::BadEscape(start.0));
                };
                let start = start.offset_char('{');
                let mut end = start;
                for _ in 0..6 {
                    match self.top_char(end) {
                        Some(ch @ ('0'..='9' | 'a'..='f' | 'A'..='F')) => end = end.offset_char(ch),
                        Some('}') => break,
                        _ => return Err(Error::BadEscape(at.0)),
                    }
                }
                let number = u32::from_str_radix(self.slice(start, end)?, 16)
                    .map_err(|_| Error::BadEscape(at.0))?;
                if self.top_char(end) != Some('}') {
                    return Err(Error::BadEscape(at.0));
                }
                let char = char::from_u32(number).ok_or(Error::BadEscape(at.0))?;
                (end.offset_char('}'), Some(char))
            }
            ch if Self::space(ch) || Self::newline(ch) => {
                let mut at = at.offset_char(ch);
                while let Some(next) = self.top_char(at) {
                    if !Self::space(next) && !Self::newline(next) {
                        break;
                    }
                    at = at.offset_char(next);
                }
                (at, None)
            }
            _ => return Err(Error::BadEscape(at.0)),
        };
        Ok((at, ch))
    }
    fn string_end(&self, pos: Pos, multi: bool, raw: usize) -> Option<Pos> {
        self.tail(pos)
            .strip_prefix(if multi { "\"\"\"" } else { "\"" })
            .and_then(|tail| tail.as_bytes().get(..raw))
            .filter(|hashes| hashes.iter().all(|&ch| ch == b'#'))
            .map(|_| {
                pos.offset_bytes(if multi { 3 } else { 1 })
                    .offset_bytes(raw)
            })
    }
    fn dedent_multiline(&self, first: Pos, mut lines: Vec<Pos>, raw: usize) -> PResult<String> {
        // an interesting thing to note is that whitespace escapes
        // can never be within an indent, as they'll consume all the indent afterwards
        // this also means that all indents are byte-for-byte exact
        let Some(last) = lines.pop() else {
            return Err(Error::ExpectedNewline(first.0));
        };
        // validate that last line is all space
        let indent = {
            let start = last;
            let mut first = None;
            let mut end = start;
            loop {
                match self.top_char(end) {
                    Some('\\') if raw == 0 => {
                        let (next, ch) = self.escape(end.offset_char('\\'))?;
                        if ch.is_some() {
                            return Err(Error::ExpectedSpace(end.0));
                        }
                        first = first.or(Some(end));
                        end = next;
                    }
                    Some('"') if self.string_end(end, true, raw).is_some() => {
                        first = first.or(Some(end));
                        break;
                    }
                    Some(ch) => {
                        if Self::space(ch) {
                            end = end.offset_char(ch);
                        } else {
                            return Err(Error::ExpectedSpace(end.0));
                        }
                    }
                    None => return Err(self.eof()),
                }
            }
            self.slice(start, first.ok_or_else(|| self.eof())?)?
        };
        lines
            .into_iter()
            .map(|start| {
                // entirely space line?
                let mut at = start;
                loop {
                    let top = self.top_char(at).ok_or_else(|| self.eof())?;
                    if Self::newline(top) {
                        return Ok(String::new());
                    } else if !Self::space(top) {
                        break;
                    }
                    at = at.offset_char(top);
                }
                if self.tail(start).starts_with(indent) {
                    let mut at = start.offset_str(indent);
                    let mut text = String::new();
                    loop {
                        match self.top_char(at) {
                            Some('\\') if raw == 0 => {
                                let (next, ch) = self.escape(at.offset_char('\\'))?;
                                at = next;
                                text.extend(ch);
                            }
                            Some(ch) => {
                                if Self::newline(ch) {
                                    break Ok(text);
                                } else {
                                    text.push(ch);
                                    at = at.offset_char(ch);
                                }
                            }
                            None => break Err(self.eof()),
                        }
                    }
                } else {
                    Err(Error::BadIndent(start.0))
                }
            })
            .collect::<PResult<Vec<_>>>()
            .map(|lines| lines.join("\n"))
    }
    /// {single, multi}-line {raw, escaped} string, starting after the first "
    fn quoted_string(&self, start: Pos, raw: usize) -> PResult<(Pos, Cow<'text, str>)> {
        if self.tail(start).starts_with("\"\"") {
            // multi-line: `newline (line newline)* indent* """`
            // line: `indent* text*`
            let mut at = start.offset_str("\"\"");
            let mut lines = Vec::<Pos>::new();
            loop {
                match self.top_char(at) {
                    Some('\\') if raw == 0 => {
                        let (next, _) = self.escape(at.offset_char('\\'))?;
                        at = next;
                    }
                    Some('"') if self.tail(at).starts_with("\"\"\"") => {
                        if let Some(next) = self.string_end(at, true, raw) {
                            break Ok((next, Cow::Owned(self.dedent_multiline(at, lines, raw)?)));
                        }
                        // more text!
                        at = at.offset_str("\"\"\"");
                    }
                    Some(ch) => {
                        if Self::newline(ch) {
                            let mut next = at.offset_char(ch);
                            if ch == '\r' && self.top_char(next) == Some('\n') {
                                at = next;
                                next = at.offset_char('\n');
                            }
                            lines.push(next);
                        } else if Self::banned(ch) {
                            return Err(Error::BannedChar(ch, at.0));
                        } else if lines.is_empty() {
                            return Err(Error::ExpectedNewline(at.0));
                        }
                        at = at.offset_char(ch);
                    }
                    None => return Err(self.eof()),
                }
            }
        } else {
            // single-line
            // none = can be borrowed
            let mut text = None::<String>;
            let mut at = start;
            loop {
                match self.top_char(at) {
                    Some('\\') if raw == 0 => {
                        let owned = match text.take() {
                            Some(owned) => owned,
                            None => self.slice(start, at)?.to_owned(),
                        };
                        let text = text.insert(owned);
                        let (next, ch) = self.escape(at.offset_char('\\'))?;
                        at = next;
                        text.extend(ch);
                    }
                    Some('"') => {
                        if let Some(next) = self.string_end(at, false, raw) {
                            break Ok((
                                next,
                                match text {
                                    Some(text) => Cow::Owned(text),
                                    None => Cow::Borrowed(self.slice(start, at)?),
                                },
                            ));
                        }
                        // more text!
                        if let Some(text) = &mut text {
                            text.push('"');
                        }
                        at = at.offset_char('"');
                    }
                    Some(ch) => {
                        if Self::newline(ch) {
                            return Err(Error::UnexpectedNewline(at.0));
                        } else if Self::banned(ch) {
                            return Err(Error::BannedChar(ch, at.0));
                        }
                        if let Some(text) = &mut text {
                            text.push(ch);
                        }
                        at = at.offset_char(ch);
                    }
                    None => return Err(self.eof()),
                }
            }
        }
    }
    /// `string | number | keyword`, may be invalid
    fn semi_value(&self, at: Pos) -> PResult<(Pos, SemiValue<'text>)> {
        match self.top_char(at) {
            Some('"') => {
                let (at, text) = self.quoted_string(at.offset_char('"'), 0)?;
                Ok((at, SemiValue::String(text)))
            }
            Some('#') => {
                let start = at;
                let mut at = at.offset_char('#');
                match self.top_char(at) {
                    Some(ch) if Self::ident(ch) => {
                        let (at, text) = self.identifier_string(at)?;
                        Ok((at, SemiValue::Keyword(text)))
                    }
                    _ => {
                        let mut raw = 1_usize;
                        while let Some('#') = self.top_char(at) {
                            at = at.offset_char('#');
                            raw = raw.checked_add(1).ok_or(Error::Overflow(at.0))?;
                        }
                        if self.top_char(at) != Some('"') {
                            return Err(Error::ExpectedString(start.0));
                        }
                        let (at, text) = self.quoted_string(at.offset_char('"'), raw)?;
                        Ok((at, SemiValue::String(text)))
                    }
                }
            }
            Some(ch) if Self::ident(ch) => {
                let (next, text) = self.identifier_string(at)?;
                Ok((
                    next,
                    if Self::number_like(text) {
                        SemiValue::Number(text)
                    } else if matches!(text, "inf" | "-inf" | "nan" | "true" | "false" | "null") {
                        return Err(Error::BadIdentifier(at.0));
                    } else {
                        SemiValue::String(Cow::Borrowed(text))
                    },
                ))
            }
            Some('}') => Err(Error::UnexpectedCloseBracket(at.0)),
            None => Err(self.eof()), // No character found, so probably EOF
            _ => Err(Error::BadIdentifier(at.0)),
        }
    }

    /// `string`
    fn string(&self, at: Pos) -> PResult<(Pos, Cow<'text, str>)> {
        let (next, value) = self.semi_value(at)?;
        match value {
            SemiValue::String(text) => Ok((next, text)),
            SemiValue::Number(_) | SemiValue::Keyword(_) => Err(Error::ExpectedString(at.0)),
        }
    }
    /// `string | number | keyword`
    fn value(&self, at: Pos) -> PResult<(Pos, Value<'text>)> {
        let (next, value) = self.semi_value(at)?;
        Ok((
            next,
            match value {
                SemiValue::String(text) => Value::String(text),
                SemiValue::Number(text) if Self::is_number(text) => Value::Number(text),
                SemiValue::Number(_) => return Err(Error::InvalidNumber(at.0)),
                SemiValue::Keyword("null") => Value::Null,
                SemiValue::Keyword("true") => Value::Bool(true),
                SemiValue::Keyword("false") => Value::Bool(false),
                SemiValue::Keyword("inf") => Value::Number("#inf"),
                SemiValue::Keyword("-inf") => Value::Number("#-inf"),
                SemiValue::Keyword("nan") => Value::Number("#nan"),
                SemiValue::Keyword(_) => return Err(Error::BadKeyword(at.0)),
            },
        ))
    }
    /// `type?`
    fn type_hint(&self, at: Pos) -> PResult<(Pos, Option<Cow<'text, str>>)> {
        if self.top_char(at) == Some('(') {
            let at = self.node_space(at.offset_char('('), false)?;
            let (at, text) = self.string(at)?;
            let at = self.node_space(at, false)?;
            if self.top_char(at) == Some(')') {
                Ok((at.offset_char(')'), Some(text)))
            } else {
                Err(Error::ExpectedCloseParen(at.0))
            }
        } else {
            Ok((at, None))
        }
    }
    /// `line-space* eob` or `line-space* slashdash? type? node-space* string`
    fn start_node(&self, at: Pos, root: bool) -> PResult<(Pos, InnerEvent<'text>)> {
        let at = self.line_space(at)?;
        if root {
            if self.top_char(at).is_none() {
                return Ok((at, InnerEvent::Done));
            }
        } else if self.top_char(at) == Some('}') {
            return Ok((at.offset_char('}'), InnerEvent::End { offset: at.0 }));
        }
        let next = self.slash_dash(at)?;
        let sd = next.is_some();
        let at = next.unwrap_or(at);
        let (at, ty) = self.type_hint(at)?;
        let at = self.node_space(at, false)?;
        let start = at.0;
        let (at, name) = self.string(at)?;
        Ok((
            at,
            InnerEvent::Node {
                sd,
                ty,
                name,
                start,
            },
        ))
    }
    fn begin_document(&self, at: Pos) -> PResult<(Pos, InnerEvent<'text>)> {
        let at = self.bom(at).unwrap_or(at);
        self.start_node(at, true)
    }
    // TODO: in the next release of KDL, this syntax changes!
    // node-space* eob -> End
    // node-space* node-terminator start-node (if !root) -> Node
    // node-space+ slashdash? node-prop-or-arg (if props) -> PropValue
    // node-space+ slashdash? { -> Begin
    fn node_item(&self, at: Pos, root: bool, props: bool) -> PResult<(Pos, InnerEvent<'text>)> {
        let first = at;
        let at = self.node_space(at, false)?;
        if root && self.top_char(at).is_none() {
            return Ok((at, InnerEvent::Done));
        }
        match self.top_char(at) {
            Some('}') if !root => {
                return Ok((at.offset_char('}'), InnerEvent::End { offset: at.0 }));
            }
            // node-terminator
            Some(';') => return self.start_node(at.offset_char(';'), root),
            Some('/') if self.top_char(at.offset_char('/')) == Some('/') => {
                let at = self.single_line_comment(at.offset_str("//"))?;
                return self.start_node(at, root);
            }
            Some(ch) if Self::newline(ch) => return self.start_node(at.offset_char(ch), root),
            _ => {}
        }
        if at.0 == first.0 {
            return Err(Error::ExpectedSpace(at.0));
        }
        let next = self.slash_dash(at)?;
        let sd = next.is_some();
        let at = next.unwrap_or(at);
        if self.top_char(at) == Some('{') {
            Ok((at.offset_char('{'), InnerEvent::Begin { sd, offset: at.0 }))
        } else if props {
            // prop/value sucks to parse, the two valid options here are:
            // - type? node-space* value
            // - string node-space* = node-space* type? node-space* value
            // which we can parse as:
            // - type node-space* value
            // - value (node-space* was already consumed)
            // - string node-space* = node-space* type? node-space* value
            // that third one comes as a tail-check of the second, only consume the space if
            // it's used
            if let (at, Some(ty)) = self.type_hint(at)? {
                let at = self.node_space(at, false)?;
                let start = at.0;
                let (at, value) = self.value(at)?;
                Ok((
                    at,
                    InnerEvent::PropValue {
                        sd,
                        ty: Some(ty),
                        key: None,
                        value,
                        start,
                    },
                ))
            } else {
                // this is a different at binding than type_hint, but it's the same value;
                // it is where both a property's key and an untyped argument begin
                let start = at.0;
                let (at, value) = self.value(at)?;
                // try for a property
                let value = match value {
                    Value::String(key) => {
                        let at = self.node_space(at, false)?;
                        if self.top_char(at) == Some('=') {
                            let at = self.node_space(at.offset_char('='), false)?;
                            let (at, ty) = self.type_hint(at)?;
                            let at = self.node_space(at, false)?;
                            let (at, real) = self.value(at)?;
                            return Ok((
                                at,
                                InnerEvent::PropValue {
                                    sd,
                                    ty,
                                    key: Some(key),
                                    value: real,
                                    start,
                                },
                            ));
                        }
                        // fail and reuse value
                        Value::String(key)
                    }
                    Value::Number(_) | Value::Bool(_) | Value::Null => value,
                };
                Ok((
                    at,
                    InnerEvent::PropValue {
                        sd,
                        ty: None,
                        key: None,
                        value,
                        start,
                    },
                ))
            }
        } else {
            Err(Error::ExpectedValue(at.0))
        }
    }
}

/// A streaming parser, is an [`Iterator`] of [`Event`]
pub(crate) struct Parser<'text> {
    grammar: Grammar<'text>,
    cursor: Pos,
    state: ParserState,
    // used in sd-filtering
    begin_valid: bool,
    // number of levels deep
    // used to determine if a } is still needed
    nest: usize,
}

impl<'text> Parser<'text> {
    /// Create a new parser from a text string
    pub(crate) fn new(text: &'text str) -> Self {
        Self {
            grammar: Grammar(text),
            cursor: Pos(0),
            state: ParserState::BeginDocument,
            begin_valid: false,
            nest: 0,
        }
    }
    fn next_event(&mut self) -> PResult<InnerEvent<'text>> {
        let event = match &mut self.state {
            ParserState::BeginDocument => {
                let (cursor, event) = self.grammar.begin_document(self.cursor)?;
                self.cursor = cursor;
                event
            }
            ParserState::NextNode => {
                let (cursor, event) = self.grammar.start_node(self.cursor, self.nest == 0)?;
                self.cursor = cursor;
                event
            }
            ParserState::NodeProps => {
                let (cursor, event) = self.grammar.node_item(self.cursor, self.nest == 0, true)?;
                self.cursor = cursor;
                event
            }
            // same as NodeProps without propvalue
            ParserState::NodeChildren => {
                let (cursor, event) = self.grammar.node_item(self.cursor, self.nest == 0, false)?;
                self.cursor = cursor;
                event
            }
            ParserState::Done => InnerEvent::Done,
        };
        self.state = match event {
            InnerEvent::Node { .. } | InnerEvent::PropValue { .. } => ParserState::NodeProps,
            InnerEvent::Begin { offset, .. } => {
                self.nest = self.nest.checked_add(1).ok_or(Error::Overflow(offset))?;
                ParserState::NextNode
            }
            InnerEvent::End { offset } => {
                self.nest = self.nest.checked_sub(1).ok_or(Error::Overflow(offset))?;
                ParserState::NodeChildren
            }
            InnerEvent::Done => ParserState::Done,
        };
        Ok(event)
    }
    fn next_real(&mut self) -> PResult<Option<Event<'text>>> {
        // sd node -> pull until node/end/finish, consume end, then loop
        // sd value -> consume & loop
        let mut next_pop = None;
        Ok(Some(loop {
            break match next_pop.take().ok_or(()).or_else(|()| self.next_event())? {
                InnerEvent::Node { sd: true, .. } => {
                    // continue until next node/end/finish
                    // next_pop is always none here so we can just take events
                    let mut depth = 0_usize;
                    next_pop = loop {
                        match self.next_event()? {
                            node @ InnerEvent::Node { sd: false, .. } if depth == 0 => {
                                break Some(node);
                            }
                            InnerEvent::Begin { .. } => {
                                depth =
                                    depth.checked_add(1).ok_or(Error::Overflow(self.cursor.0))?;
                            }
                            end @ InnerEvent::End { .. } => match depth.checked_sub(1) {
                                Some(next) => {
                                    depth = next;
                                    if depth == 0 {
                                        break None;
                                    }
                                }
                                None => break Some(end),
                            },
                            InnerEvent::Done => break Some(InnerEvent::Done),
                            InnerEvent::Node { .. } | InnerEvent::PropValue { .. } => {}
                        }
                    };
                    continue;
                }
                InnerEvent::Begin { sd: true, .. } => {
                    let mut depth = 0_usize;
                    loop {
                        match self.next_event()? {
                            InnerEvent::Begin { .. } => {
                                depth =
                                    depth.checked_add(1).ok_or(Error::Overflow(self.cursor.0))?;
                            }
                            InnerEvent::End { .. } => {
                                if let Some(next) = depth.checked_sub(1) {
                                    depth = next;
                                } else {
                                    break;
                                }
                            }
                            InnerEvent::Node { .. } | InnerEvent::PropValue { .. } => {}
                            // Unreachable inside a block, but ending here beats looping on Done forever.
                            InnerEvent::Done => return Err(self.grammar.eof()),
                        }
                    }
                    continue;
                }
                InnerEvent::Node {
                    sd: false,
                    ty,
                    name,
                    start,
                } => {
                    self.begin_valid = true;
                    Event::Node { ty, name, start }
                }
                InnerEvent::PropValue { sd: true, .. } => continue,
                InnerEvent::PropValue {
                    sd: false,
                    ty,
                    key,
                    value,
                    start,
                } => Event::Entry {
                    ty,
                    key,
                    value,
                    start,
                },
                InnerEvent::Begin { sd: false, offset } => {
                    if self.begin_valid {
                        Event::Begin { offset }
                    } else {
                        return Err(Error::MultipleChildren(offset));
                    }
                }
                InnerEvent::End { offset } => {
                    self.begin_valid = false;
                    Event::End { offset }
                }
                InnerEvent::Done => return Ok(None),
            };
        }))
    }
}

impl<'text> Iterator for Parser<'text> {
    type Item = PResult<Event<'text>>;
    fn next(&mut self) -> Option<Self::Item> {
        let event = self.next_real();
        // this is a terrible place to put it but oh well
        if event.is_err() {
            self.state = ParserState::Done;
        }
        event.transpose()
    }
}
