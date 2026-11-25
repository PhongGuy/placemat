//! The loader: untrusted bytes pass the byte cap, strict UTF-8 and the bounded KDL reader, then
//! the tree maps onto the model element by element, each refusal carrying its line and column.

use std::collections::{BTreeMap, BTreeSet};

use super::model::Unknown;
use super::write::{entry_text, node_text};
use super::{
    ANCHOR_MAX, Align, Anchor, Arrow, ArrowEnd, Board, Boxed, COORD_MAX, Colour, Dash, Element,
    ElementId, Fill, FillStyle, Font, FontFamily, Head, Heads, Kind, LIMITS, POINT_MAX, Path,
    Point, ROUGHNESS_MAX, Seed, Size, Stroke, TENTHS_MAX, Text, VERSION, ZKey,
};
use crate::kdl::{self, Entry, Node, ReadError, ReadErrorKind, ReadLimits, Value};

/// Unknown entries found on an element's known lines, by line name, in normal form.
type Extra = BTreeMap<String, Vec<String>>;

/// A loaded board with what the load repaired and whether it may be saved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Loaded {
    /// The board.
    pub board: Board,
    /// What was repaired or worth telling the user, in a fixed order.
    pub warnings: Vec<Warning>,
    /// Whether this build may write the board back.
    pub access: Access,
}

/// Whether a loaded board may be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Access {
    /// The board is at a version this build writes.
    ReadWrite,
    /// The board is newer than this build; it can be viewed but not saved.
    ReadOnly {
        /// The version the file declares.
        found: u32,
    },
}

/// One end of an arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum End {
    /// The start.
    From,
    /// The finish.
    To,
}

/// Something a load repaired or noticed, without refusing the board.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// An arrow end was bound to an id the board does not hold and was freed.
    DanglingBinding {
        /// The arrow.
        arrow: ElementId,
        /// Which end.
        end: End,
        /// The missing target.
        target: ElementId,
    },
    /// A later element repeated an id and was renamed.
    DuplicateId {
        /// The repeated id.
        original: ElementId,
        /// The id it now has.
        renamed: ElementId,
    },
    /// A seed above [`Seed::MAX`] was clamped to it.
    SeedClamped {
        /// The element.
        id: ElementId,
    },
    /// A malformed z key was replaced by the empty key, which paints last.
    InvalidZ {
        /// The element.
        id: ElementId,
    },
    /// The file is larger than [`LIMITS`]`.team_bytes`, too large to share comfortably.
    OverTeamLimit {
        /// The file's size.
        bytes: usize,
    },
}

/// Why bytes were not loaded as a board.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The bytes are longer than [`LIMITS`]`.hard_bytes`.
    #[error("board is over the 16 MiB limit")]
    TooLarge,
    /// The bytes are not UTF-8.
    #[error("board is not UTF-8 text")]
    NotUtf8,
    /// The text does not start with a well-formed `placemat-board` header.
    #[error("not a Placemat board")]
    NotABoard,
    /// The text is not KDL the reader accepts.
    #[error("board syntax error at line {line}, column {col}: {msg}")]
    Syntax {
        /// 1-based line.
        line: u32,
        /// 1-based column, in characters.
        col: u32,
        /// What the reader found.
        msg: String,
    },
    /// The board holds more than [`LIMITS`] or the reader's bounds allow.
    #[error("board {limit} limit exceeded")]
    OverLimit {
        /// The exceeded [`super::Limits`] field, or `nodes` or `entries` for the reader's bounds.
        limit: &'static str,
        /// The element that crossed a per-element or running limit.
        id: Option<ElementId>,
    },
    /// A node, line or value breaks the board format.
    #[error("invalid board at line {line}, column {col}: {msg}")]
    Invalid {
        /// The element it belongs to, when its id was readable.
        id: Option<ElementId>,
        /// 1-based line.
        line: u32,
        /// 1-based column, in characters.
        col: u32,
        /// What is wrong.
        msg: String,
    },
}

/// Loads board bytes into the model, never panicking. A seed past [`Seed::MAX`] is clamped and
/// a malformed z key is replaced by the empty key (written `z ""`, painted last), each with a
/// warning.
///
/// # Errors
///
/// Returns a [`LoadError`] for oversized, non-UTF-8, malformed or over-limit input.
pub fn load(bytes: &[u8]) -> Result<Loaded, LoadError> {
    if bytes.len() > LIMITS.hard_bytes {
        return Err(LoadError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| LoadError::NotUtf8)?;
    let nodes = kdl::parse(text, &read_limits()).map_err(|e| read_error(text, e))?;
    let (header, body) = nodes.split_first().ok_or(LoadError::NotABoard)?;
    let version = version(header).ok_or(LoadError::NotABoard)?;
    let header = header
        .entries
        .iter()
        .filter(|e| e.key.as_deref() != Some("version"))
        .map(entry_text)
        .collect();
    let (mapped, keyed, unkeyed) = map(text, body)?;
    let board =
        Board::with_unknown(version, header, keyed, unkeyed).ok_or(LoadError::OverLimit {
            limit: "unknown_bytes_total",
            id: None,
        })?;
    let mut warnings: Vec<Warning> = mapped
        .iter()
        .filter(|m| m.seed_clamped)
        .map(|m| Warning::SeedClamped {
            id: m.element.id.clone(),
        })
        .collect();
    warnings.extend(
        mapped
            .iter()
            .filter(|m| m.z_invalid)
            .map(|m| Warning::InvalidZ {
                id: m.element.id.clone(),
            }),
    );
    let board = assemble(text, board, mapped)?;
    if bytes.len() > LIMITS.team_bytes {
        warnings.push(Warning::OverTeamLimit { bytes: bytes.len() });
    }
    let access = if version > VERSION {
        Access::ReadOnly { found: version }
    } else {
        Access::ReadWrite
    };
    Ok(Loaded {
        board,
        warnings,
        access,
    })
}

/// The reader's bounds for a board: an element block holds lines, so depth 4 leaves room for
/// unknown nesting while the counts stay well above a board at its limits.
fn read_limits() -> ReadLimits {
    ReadLimits::default()
        .with_max_depth(4)
        .with_max_nodes(1_000_000)
        .with_max_entries(4_000_000)
        .with_max_string_bytes(1 << 20)
}

fn read_error(text: &str, e: ReadError) -> LoadError {
    let limit = |limit| LoadError::OverLimit { limit, id: None };
    match e.kind {
        ReadErrorKind::Syntax | ReadErrorKind::UnexpectedEof | ReadErrorKind::TooDeep => {
            let (line, col) = kdl::line_col(text, e.offset);
            LoadError::Syntax {
                line,
                col,
                msg: e.kind.to_string(),
            }
        }
        ReadErrorKind::TooManyNodes => limit("nodes"),
        ReadErrorKind::TooManyEntries => limit("entries"),
        ReadErrorKind::StringTooLong => limit("string_bytes_each"),
    }
}

/// The header's version: a bare `placemat-board` node with exactly one `version` property
/// holding a plain non-negative integer that fits `u32`.
fn version(header: &Node) -> Option<u32> {
    if header.name != "placemat-board" || header.ty.is_some() || header.children.is_some() {
        return None;
    }
    let mut versions = header
        .entries
        .iter()
        .filter(|e| e.key.as_deref() == Some("version"));
    let entry = versions.next()?;
    if versions.next().is_some() || entry.ty.is_some() {
        return None;
    }
    match &entry.value {
        Value::Number(n) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            n.parse().ok()
        }
        Value::Number(_) | Value::String(_) | Value::Bool(_) | Value::Null => None,
    }
}

/// A refusal inside one element: the byte offset it points at and what is wrong.
struct Bad {
    at: u32,
    msg: String,
}

fn bad(at: u32, msg: impl Into<String>) -> Bad {
    Bad {
        at,
        msg: msg.into(),
    }
}

fn invalid(text: &str, id: Option<ElementId>, bad: Bad) -> LoadError {
    let (line, col) = kdl::line_col(text, bad.at);
    LoadError::Invalid {
        id,
        line,
        col,
        msg: bad.msg,
    }
}

/// One element as read, with where it starts and what the load repaired in it.
struct Mapped {
    element: Element,
    start: u32,
    seed_clamped: bool,
    z_invalid: bool,
}

/// The top-level nodes after the header: elements, unknown nodes keyed by their first string
/// argument, and unkeyed unknown nodes.
type Mapping = (Vec<Mapped>, BTreeMap<String, Vec<String>>, Vec<String>);

/// Maps every top-level node after the header in file order, stopping at the first element
/// past the count limit so a huge file is not mapped in full first. Unknown nodes are kept in
/// normal form.
fn map(text: &str, body: &[Node]) -> Result<Mapping, LoadError> {
    let mut mapped = Vec::new();
    let (mut keyed, mut unkeyed) = (BTreeMap::<String, Vec<String>>::new(), Vec::new());
    let mut seen = BTreeSet::new();
    for node in body {
        let Some(lines) = lines_of(&node.name) else {
            match node.entries.first() {
                Some(Entry {
                    key: None,
                    value: Value::String(key),
                    ..
                }) => keyed.entry(key.clone()).or_default().push(node_text(node)),
                _ => unkeyed.push(node_text(node)),
            }
            continue;
        };
        if mapped.len() >= LIMITS.elements {
            return Err(LoadError::OverLimit {
                limit: "elements",
                id: None,
            });
        }
        let id = element_id(node).map_err(|b| invalid(text, None, b))?;
        if !seen.insert(id.clone()) {
            return Err(invalid(text, Some(id), bad(node.start, "duplicate id")));
        }
        let m = element(node, id.clone(), lines).map_err(|b| invalid(text, Some(id), b))?;
        mapped.push(m);
    }
    Ok((mapped, keyed, unkeyed))
}

/// Fills the board through [`Board::insert`], so its running totals match its elements and
/// every total limit is checked once, naming the element that crossed it. Unknown bytes are a
/// whole-board budget, so that refusal names no element.
fn assemble(text: &str, mut board: Board, mapped: Vec<Mapped>) -> Result<Board, LoadError> {
    for m in mapped {
        let id = m.element.id.clone();
        board.insert(m.element).map_err(|r| match r.limit {
            "range" => invalid(text, Some(id.clone()), bad(m.start, "value out of range")),
            "unknown_bytes_total" => LoadError::OverLimit {
                limit: "unknown_bytes_total",
                id: None,
            },
            limit => LoadError::OverLimit {
                limit,
                id: Some(id.clone()),
            },
        })?;
    }
    Ok(board)
}

/// The line names each kind holds, in canonical order.
fn lines_of(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "rect" | "ellipse" | "diamond" => &["at", "seed", "size", "fill", "stroke", "label", "z"],
        "line" | "freedraw" => &["at", "seed", "points", "stroke", "z"],
        "arrow" => &["from", "seed", "to", "head", "stroke", "label", "z"],
        "text" => &["at", "seed", "content", "font", "stroke", "z"],
        _ => return None,
    })
}

/// The element's id: its one positional argument, a valid id string.
fn element_id(node: &Node) -> Result<ElementId, Bad> {
    let id = match node.entries.first() {
        Some(Entry {
            key: None,
            ty: None,
            value: Value::String(s),
            ..
        }) => ElementId::new(s),
        _ => None,
    };
    id.ok_or_else(|| bad(node.start, "expected a valid element id"))
}

/// One element node mapped onto the model, its lines found by name and each read in turn.
/// Entries after the id, unknown lines and unknown entries on known lines are kept.
fn element(node: &Node, id: ElementId, names: &'static [&'static str]) -> Result<Mapped, Bad> {
    if node.ty.is_some() {
        return Err(bad(node.start, "type annotation on an element"));
    }
    let lines = Lines::find(node, names)?;
    let x = &mut Extra::new();
    let (seed, seed_clamped) = seed(lines.need("seed")?, x)?;
    let stroke = stroke(lines.need("stroke")?, x)?;
    let (z, z_invalid) = z(lines.need("z")?, x)?;
    let label = lines.get("label").map(|l| label(l, x)).transpose()?;
    let boxed = |x: &mut Extra| -> Result<Boxed, Bad> {
        let mut b = Boxed::new(point(lines.need("at")?, x)?, size(lines.need("size")?, x)?);
        b.fill = fill(lines.need("fill")?, x)?;
        b.label = label.clone();
        Ok(b)
    };
    let path = |x: &mut Extra| -> Result<Path, Bad> {
        Ok(Path::new(
            point(lines.need("at")?, x)?,
            points(lines.need("points")?, x)?,
        ))
    };
    let kind = match node.name.as_str() {
        "rect" => Kind::Rect(boxed(x)?),
        "ellipse" => Kind::Ellipse(boxed(x)?),
        "diamond" => Kind::Diamond(boxed(x)?),
        "line" => Kind::Line(path(x)?),
        "freedraw" => Kind::Freedraw(path(x)?),
        "arrow" => {
            let mut a = Arrow::new(
                arrow_end(lines.need("from")?, x)?,
                arrow_end(lines.need("to")?, x)?,
            );
            a.heads = heads(lines.need("head")?, x)?;
            a.label = label;
            Kind::Arrow(a)
        }
        _ => {
            let mut t = Text::new(
                point(lines.need("at")?, x)?,
                content(lines.need("content")?, x)?,
            );
            t.font = font(lines.need("font")?, x)?;
            Kind::Text(t)
        }
    };
    let mut element = Element::new(id, seed, z, kind);
    element.stroke = stroke;
    element.unknown = Unknown {
        opener: node.entries.iter().skip(1).map(entry_text).collect(),
        entries: std::mem::take(x),
        lines: lines.unknown,
    };
    Ok(Mapped {
        element,
        start: node.start,
        seed_clamped,
        z_invalid,
    })
}

/// An element's lines by name, with the lines outside the kind's table kept in normal form; a
/// known line seen twice is refused.
struct Lines<'n> {
    element: &'n Node,
    names: &'static [&'static str],
    found: Vec<Option<&'n Node>>,
    unknown: Vec<String>,
}

impl<'n> Lines<'n> {
    fn find(element: &'n Node, names: &'static [&'static str]) -> Result<Self, Bad> {
        let mut found = vec![None; names.len()];
        let mut unknown = Vec::new();
        for line in element.children.iter().flatten() {
            let Some(slot) = names
                .iter()
                .position(|n| *n == line.name)
                .and_then(|i| found.get_mut(i))
            else {
                unknown.push(node_text(line));
                continue;
            };
            if slot.replace(line).is_some() {
                return Err(bad(line.start, format!("repeated `{}` line", line.name)));
            }
        }
        Ok(Self {
            element,
            names,
            found,
            unknown,
        })
    }

    fn get(&self, name: &str) -> Option<&'n Node> {
        let i = self.names.iter().position(|n| *n == name)?;
        self.found.get(i).copied().flatten()
    }

    fn need(&self, name: &str) -> Result<&'n Node, Bad> {
        self.get(name)
            .ok_or_else(|| bad(self.element.start, format!("missing `{name}` line")))
    }
}

/// Splits a line into its first `max_args` positional arguments and the properties named by
/// `keys`, in that order; later arguments and other properties go to `x` as unknown entries.
/// Every known-line rule lives here: no annotation, no block, no repeated or missing property.
fn split<'n>(
    line: &'n Node,
    keys: &[&str],
    max_args: usize,
    x: &mut Extra,
) -> Result<(Vec<&'n Entry>, Vec<&'n Entry>), Bad> {
    if line.ty.is_some() {
        return Err(bad(line.start, "type annotation on a line"));
    }
    if line.children.is_some() {
        return Err(bad(line.start, "a line takes no block"));
    }
    let mut args = Vec::new();
    let mut props: Vec<Option<&Entry>> = vec![None; keys.len()];
    let mut unknown = Vec::new();
    for entry in &line.entries {
        let known_arg = entry.key.is_none() && args.len() < max_args;
        let prop = entry.key.as_ref().and_then(|key| {
            let slot = keys.iter().position(|k| k == key);
            slot.and_then(|i| props.get_mut(i)).map(|slot| (key, slot))
        });
        if !known_arg && prop.is_none() {
            unknown.push(entry_text(entry));
            continue;
        }
        if entry.ty.is_some() {
            return Err(bad(entry.start, "type annotation on a value"));
        }
        match prop {
            Some((key, slot)) => {
                if slot.replace(entry).is_some() {
                    return Err(bad(entry.start, format!("repeated property `{key}`")));
                }
            }
            None => args.push(entry),
        }
    }
    let props = props
        .into_iter()
        .zip(keys)
        .map(|(p, k)| p.ok_or_else(|| bad(line.start, format!("missing property `{k}`"))))
        .collect::<Result<_, _>>()?;
    if !unknown.is_empty() {
        x.insert(line.name.clone(), unknown);
    }
    Ok((args, props))
}

/// [`split`] for a line with exactly `N` arguments and `K` properties.
fn fixed_split<'n, const N: usize, const K: usize>(
    line: &'n Node,
    keys: [&str; K],
    x: &mut Extra,
) -> Result<([&'n Entry; N], [&'n Entry; K]), Bad> {
    let (args, props) = split(line, &keys, N, x)?;
    let args = args
        .try_into()
        .map_err(|_| bad(line.start, "missing value"))?;
    let props = props
        .try_into()
        .map_err(|_| bad(line.start, "missing property"))?;
    Ok((args, props))
}

/// Refuses a key that belongs to another form of this line, such as `style` on `fill none`,
/// which would otherwise be kept as unknown data.
fn known_key_in_wrong_form(line: &Node, keys: &[&str]) -> Result<(), Bad> {
    match line
        .entries
        .iter()
        .find(|e| e.key.as_deref().is_some_and(|k| keys.contains(&k)))
    {
        Some(e) => Err(bad(e.start, "property not allowed in this form")),
        None => Ok(()),
    }
}

/// A number in `[+-]?[0-9]+(\.[0-9]+)?` with at most `digits` fraction digits, scaled by
/// `10^digits`. Exponents, radix prefixes, underscores and keywords are refused.
fn decimal(text: &str, digits: u32) -> Option<i128> {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, frac) = match body.split_once('.') {
        Some((w, f)) if !f.is_empty() => (w, f),
        Some(_) => return None,
        None => (body, ""),
    };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !all_digits(whole) || !all_digits(frac) {
        return None;
    }
    let pad = digits.checked_sub(u32::try_from(frac.len()).ok()?)?;
    let frac = if frac.is_empty() {
        0
    } else {
        frac.parse::<i128>().ok()?
    };
    let scaled = whole
        .parse::<i128>()
        .ok()?
        .checked_mul(10_i128.pow(digits))?
        .checked_add(frac.checked_mul(10_i128.pow(pad))?)?;
    Some(if negative { -scaled } else { scaled })
}

/// The entry as a number with `digits` fraction digits, scaled, within `lo..=hi`.
fn number<T: TryFrom<i128>>(e: &Entry, digits: u32, lo: i128, hi: i128) -> Result<T, Bad> {
    let what = if digits == 0 {
        "an integer"
    } else {
        "a fixed-point number"
    };
    let n = match &e.value {
        Value::Number(s) => decimal(s, digits),
        Value::String(_) | Value::Bool(_) | Value::Null => None,
    }
    .ok_or_else(|| bad(e.start, format!("expected {what}")))?;
    if !(lo..=hi).contains(&n) {
        return Err(bad(e.start, "value out of range"));
    }
    T::try_from(n).map_err(|_| bad(e.start, "value out of range"))
}

fn coord(e: &Entry) -> Result<i32, Bad> {
    number(e, 0, (-COORD_MAX).into(), COORD_MAX.into())
}

fn tenths(e: &Entry, max: u16) -> Result<u16, Bad> {
    number(e, 1, 0, max.into())
}

fn string(e: &Entry) -> Result<&str, Bad> {
    match &e.value {
        Value::String(s) => Ok(s),
        Value::Number(_) | Value::Bool(_) | Value::Null => Err(bad(e.start, "expected a string")),
    }
}

fn colour(e: &Entry) -> Result<Colour, Bad> {
    Colour::parse(string(e)?).ok_or_else(|| bad(e.start, "expected a colour"))
}

/// The entry as one of a closed set of words.
fn word<T: Copy>(e: &Entry, table: &[(&str, T)]) -> Result<T, Bad> {
    let s = string(e)?;
    table
        .iter()
        .find(|(w, _)| *w == s)
        .map(|(_, v)| *v)
        .ok_or_else(|| bad(e.start, format!("unknown value `{s}`")))
}

fn point(line: &Node, x: &mut Extra) -> Result<Point, Bad> {
    let ([px, py], []) = fixed_split(line, [], x)?;
    Ok(Point::new(coord(px)?, coord(py)?))
}

fn size(line: &Node, x: &mut Extra) -> Result<Size, Bad> {
    let p = point(line, x)?;
    Ok(Size::new(p.x, p.y))
}

/// The seed and whether it was clamped: any integer fitting `u64` loads, values above
/// [`Seed::MAX`] clamp to it.
fn seed(line: &Node, x: &mut Extra) -> Result<(Seed, bool), Bad> {
    let ([n], []) = fixed_split(line, [], x)?;
    let n: u64 = number(n, 0, 0, u64::MAX.into())?;
    let seed = Seed::clamped(u32::try_from(n).unwrap_or(u32::MAX));
    Ok((seed, u64::from(seed.get()) != n))
}

fn stroke(line: &Node, x: &mut Extra) -> Result<Stroke, Bad> {
    let ([c], [width, roughness, dash]) = fixed_split(line, ["width", "roughness", "dash"], x)?;
    let dashes = [
        ("solid", Dash::Solid),
        ("dashed", Dash::Dashed),
        ("dotted", Dash::Dotted),
    ];
    Ok(Stroke {
        colour: colour(c)?,
        width: tenths(width, TENTHS_MAX)?,
        roughness: tenths(roughness, ROUGHNESS_MAX)?,
        dash: word(dash, &dashes)?,
    })
}

/// The z key and whether it was malformed; a malformed string becomes the empty key.
fn z(line: &Node, x: &mut Extra) -> Result<(ZKey, bool), Bad> {
    let ([k], []) = fixed_split(line, [], x)?;
    Ok(match ZKey::new(string(k)?) {
        Some(z) => (z, false),
        None => (ZKey::empty(), true),
    })
}

fn label(line: &Node, x: &mut Extra) -> Result<String, Bad> {
    content(line, x)
}

fn content(line: &Node, x: &mut Extra) -> Result<String, Bad> {
    let ([s], []) = fixed_split(line, [], x)?;
    Ok(string(s)?.to_owned())
}

/// `fill none`, or a colour with its style.
fn fill(line: &Node, x: &mut Extra) -> Result<Fill, Bad> {
    if let Some(Entry {
        key: None,
        value: Value::String(s),
        ..
    }) = line.entries.first()
        && s == "none"
    {
        known_key_in_wrong_form(line, &["style"])?;
        fixed_split::<1, 0>(line, [], x)?;
        return Ok(Fill::None);
    }
    let ([c], [style]) = fixed_split(line, ["style"], x)?;
    let styles = [
        ("hachure", FillStyle::Hachure),
        ("cross-hatch", FillStyle::CrossHatch),
        ("solid", FillStyle::Solid),
        ("zigzag", FillStyle::Zigzag),
    ];
    Ok(Fill::Colour {
        colour: colour(c)?,
        style: word(style, &styles)?,
    })
}

/// Path points: pairs of fixed-point tenths relative to the path's origin.
fn points(line: &Node, x: &mut Extra) -> Result<Vec<Point>, Bad> {
    let (args, _) = split(line, &[], usize::MAX, x)?;
    let (pairs, []) = args.as_chunks::<2>() else {
        return Err(bad(line.start, "points come in pairs"));
    };
    let tenth = |e: &Entry| number(e, 1, (-POINT_MAX).into(), POINT_MAX.into());
    pairs
        .iter()
        .map(|[x, y]| Ok(Point::new(tenth(x)?, tenth(y)?)))
        .collect()
}

/// A bound end (`"id" x=.. y=..`) when the first argument is a string, else a free point.
fn arrow_end(line: &Node, x: &mut Extra) -> Result<ArrowEnd, Bad> {
    if !matches!(
        line.entries.first().map(|e| &e.value),
        Some(Value::String(_))
    ) {
        known_key_in_wrong_form(line, &["x", "y"])?;
        return point(line, x).map(ArrowEnd::Free);
    }
    let ([target], [ax, ay]) = fixed_split(line, ["x", "y"], x)?;
    let id = ElementId::new(string(target)?).ok_or_else(|| bad(target.start, "expected an id"))?;
    let fraction = |e: &Entry| number(e, 4, 0, ANCHOR_MAX.into());
    Ok(ArrowEnd::Bound {
        target: id,
        anchor: Anchor::new(fraction(ax)?, fraction(ay)?),
    })
}

fn heads(line: &Node, x: &mut Extra) -> Result<Heads, Bad> {
    let ([], [start, end]) = fixed_split(line, ["start", "end"], x)?;
    let table = [
        ("none", Head::None),
        ("arrow", Head::Arrow),
        ("triangle", Head::Triangle),
        ("bar", Head::Bar),
        ("dot", Head::Dot),
    ];
    Ok(Heads {
        start: word(start, &table)?,
        end: word(end, &table)?,
    })
}

fn font(line: &Node, x: &mut Extra) -> Result<Font, Bad> {
    let ([], [family, size, align]) = fixed_split(line, ["family", "size", "align"], x)?;
    let families = [
        ("hand", FontFamily::Hand),
        ("normal", FontFamily::Normal),
        ("code", FontFamily::Code),
    ];
    let aligns = [
        ("left", Align::Left),
        ("center", Align::Center),
        ("right", Align::Right),
    ];
    Ok(Font {
        family: word(family, &families)?,
        size: tenths(size, TENTHS_MAX)?,
        align: word(align, &aligns)?,
    })
}
