//! KDL v2 reading: [`parse`] turns any text into a bounded tree or a byte-positioned
//! [`ReadError`], never panicking, recursing per level or printing.
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::arithmetic_side_effects
)]

mod lexer;

use lexer::{Event, Parser};

/// The bounds a document must stay within. `max_nodes` and `max_entries` count the whole
/// document; `max_string_bytes` caps each name, key, type, string value and number text.
///
/// The derived `Clone`, `Debug` and `Drop` of [`Node`] recurse once per level, so `max_depth`
/// also bounds their stack use: they are tested to 150 levels (`Clone`, `Debug`) and 1,000
/// (`Drop`) on a 256 KiB stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReadLimits {
    /// Most child blocks open at once; 0 refuses any block.
    pub max_depth: u16,
    /// Most nodes in the document.
    pub max_nodes: u32,
    /// Most arguments and properties in the document.
    pub max_entries: u32,
    /// Most bytes in any one string or number text.
    pub max_string_bytes: u32,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 1_000_000,
            max_entries: 10_000_000,
            max_string_bytes: 16 << 20,
        }
    }
}

impl ReadLimits {
    /// These limits with `max_depth` replaced.
    #[must_use]
    pub const fn with_max_depth(self, max_depth: u16) -> Self {
        Self { max_depth, ..self }
    }

    /// These limits with `max_nodes` replaced.
    #[must_use]
    pub const fn with_max_nodes(self, max_nodes: u32) -> Self {
        Self { max_nodes, ..self }
    }

    /// These limits with `max_entries` replaced.
    #[must_use]
    pub const fn with_max_entries(self, max_entries: u32) -> Self {
        Self {
            max_entries,
            ..self
        }
    }

    /// These limits with `max_string_bytes` replaced.
    #[must_use]
    pub const fn with_max_string_bytes(self, max_string_bytes: u32) -> Self {
        Self {
            max_string_bytes,
            ..self
        }
    }
}

/// A KDL node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Node {
    /// The type annotation, if any.
    pub ty: Option<String>,
    /// The node name.
    pub name: String,
    /// Byte offset of the name, after any type annotation.
    pub start: u32,
    /// Arguments and properties in source order.
    pub entries: Vec<Entry>,
    /// The child block, if the node has one (`Some(vec![])` for `{}`).
    pub children: Option<Vec<Node>>,
}

/// An argument (no key) or a property (with a key).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Entry {
    /// The property key; `None` for an argument.
    pub key: Option<String>,
    /// The value's type annotation, if any.
    pub ty: Option<String>,
    /// The value.
    pub value: Value,
    /// Byte offset of a property's key, or of an argument's value after its type annotation.
    pub start: u32,
}

/// A KDL value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Value {
    /// A string, unescaped.
    String(String),
    /// A number exactly as written, including `#inf`, `#-inf` and `#nan`.
    Number(String),
    /// `#true` or `#false`.
    Bool(bool),
    /// `#null`.
    Null,
}

/// Why and where reading stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error("{kind} at byte {offset}")]
pub struct ReadError {
    /// What went wrong.
    pub kind: ReadErrorKind,
    /// Byte offset in the source text.
    pub offset: u32,
}

/// The kinds of [`ReadError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReadErrorKind {
    /// The text is not valid KDL v2.
    #[error("syntax error")]
    Syntax,
    /// The text ended inside a construct.
    #[error("unexpected end of input")]
    UnexpectedEof,
    /// More child blocks were open at once than `max_depth`.
    #[error("blocks nested too deep")]
    TooDeep,
    /// The document has more than `max_nodes` nodes.
    #[error("too many nodes")]
    TooManyNodes,
    /// The document has more than `max_entries` entries.
    #[error("too many entries")]
    TooManyEntries,
    /// A string or number text is longer than `max_string_bytes`.
    #[error("string too long")]
    StringTooLong,
}

// Offsets fit: `parse` refuses text longer than `u32::MAX` bytes up front.
fn narrow(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

fn err(kind: ReadErrorKind, offset: usize) -> ReadError {
    ReadError {
        kind,
        offset: narrow(offset),
    }
}

/// Parses `text` as a KDL v2 document within `limits`.
///
/// # Errors
///
/// Returns the first syntax error, premature end, or limit exceeded, at its byte offset.
pub fn parse(text: &str, limits: &ReadLimits) -> Result<Vec<Node>, ReadError> {
    if u32::try_from(text.len()).is_err() {
        return Err(err(ReadErrorKind::StringTooLong, 0));
    }
    // One level per open block; a block's nodes are linked to their parent only when it
    // closes, so an early return drops flat vectors and never walks a deep tree.
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    let (mut nodes, mut entries) = (0_u32, 0_u32);
    let max_len = usize::try_from(limits.max_string_bytes).unwrap_or(usize::MAX);
    let too_long = |s: Option<&str>| s.is_some_and(|s| s.len() > max_len);
    for event in Parser::new(text) {
        let event = event.map_err(|e| {
            let kind = if matches!(e, lexer::Error::UnexpectedEof(_)) {
                ReadErrorKind::UnexpectedEof
            } else {
                ReadErrorKind::Syntax
            };
            err(kind, e.offset())
        })?;
        match event {
            Event::Node { ty, name, start } => {
                if nodes >= limits.max_nodes {
                    return Err(err(ReadErrorKind::TooManyNodes, start));
                }
                if too_long(ty.as_deref()) || too_long(Some(&name)) {
                    return Err(err(ReadErrorKind::StringTooLong, start));
                }
                nodes = nodes.saturating_add(1);
                let node = Node {
                    ty: ty.map(Into::into),
                    name: name.into(),
                    start: narrow(start),
                    entries: Vec::new(),
                    children: None,
                };
                let Some(level) = stack.last_mut() else {
                    return Err(err(ReadErrorKind::Syntax, start));
                };
                level.push(node);
            }
            Event::Entry {
                key,
                ty,
                value,
                start,
            } => {
                if entries >= limits.max_entries {
                    return Err(err(ReadErrorKind::TooManyEntries, start));
                }
                let value = match value {
                    lexer::Value::String(s) => Value::String(s.into()),
                    lexer::Value::Number(n) => Value::Number(n.to_owned()),
                    lexer::Value::Bool(b) => Value::Bool(b),
                    lexer::Value::Null => Value::Null,
                };
                let text = match &value {
                    Value::String(s) | Value::Number(s) => Some(s.as_str()),
                    Value::Bool(_) | Value::Null => None,
                };
                if too_long(key.as_deref()) || too_long(ty.as_deref()) || too_long(text) {
                    return Err(err(ReadErrorKind::StringTooLong, start));
                }
                entries = entries.saturating_add(1);
                let entry = Entry {
                    key: key.map(Into::into),
                    ty: ty.map(Into::into),
                    value,
                    start: narrow(start),
                };
                let Some(node) = stack.last_mut().and_then(|level| level.last_mut()) else {
                    return Err(err(ReadErrorKind::Syntax, start));
                };
                node.entries.push(entry);
            }
            Event::Begin { offset } => {
                if stack.len() > usize::from(limits.max_depth) {
                    return Err(err(ReadErrorKind::TooDeep, offset));
                }
                stack.push(Vec::new());
            }
            Event::End { offset } => {
                let (Some(children), Some(parent)) = (stack.pop(), stack.last_mut()) else {
                    return Err(err(ReadErrorKind::Syntax, offset));
                };
                let Some(node) = parent.last_mut() else {
                    return Err(err(ReadErrorKind::Syntax, offset));
                };
                node.children = Some(children);
            }
        }
    }
    match <[_; 1]>::try_from(stack) {
        Ok([root]) => Ok(root),
        Err(_) => Err(err(ReadErrorKind::UnexpectedEof, text.len())),
    }
}

/// The 1-based line and column of byte `offset` in `text`. Columns count chars; every KDL
/// newline starts a line, CRLF once. An offset past the end clamps to the end.
#[must_use]
pub fn line_col(text: &str, offset: u32) -> (u32, u32) {
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    let (mut line, mut col, mut prev) = (1_u32, 1_u32, '\0');
    for (at, ch) in text.char_indices() {
        if at >= offset {
            break;
        }
        if prev == '\r' && ch == '\n' {
            // The second half of a CRLF: the line already advanced at the `\r`.
        } else if lexer::is_newline(ch) {
            line = line.saturating_add(1);
            col = 1;
        } else {
            col = col.saturating_add(1);
        }
        prev = ch;
    }
    (line, col)
}
