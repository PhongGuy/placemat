//! Paint-order keys and their order.

use std::cmp::Ordering;

/// Longest valid key, in characters.
const MAX_LEN: usize = 64;

/// A paint-order key: 1 to 64 base62 characters, not ending in `0`, compared by bytes. A key
/// loaded malformed is held as the empty key, written `z ""`, and sorts after every valid key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ZKey(String);

impl ZKey {
    /// The key spelled `s`, or `None` when `s` is not 1 to 64 base62 characters or ends in `0`.
    pub fn new(s: &str) -> Option<Self> {
        let valid = (1..=MAX_LEN).contains(&s.len())
            && s.bytes().all(|b| b.is_ascii_alphanumeric())
            && !s.ends_with('0');
        valid.then(|| Self(s.to_owned()))
    }

    /// The malformed placeholder a load puts in place of an unreadable key.
    pub(crate) fn empty() -> Self {
        Self(String::new())
    }

    /// The key's text; empty for a key loaded malformed.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is a well-formed key rather than the malformed placeholder.
    pub fn is_valid(&self) -> bool {
        !self.0.is_empty()
    }
}

// Valid keys order by their bytes; the malformed key goes last so it never paints under a
// well-formed one. Comparing the flag first keeps the order consistent with `Eq`.
impl Ord for ZKey {
    fn cmp(&self, other: &Self) -> Ordering {
        (!self.is_valid(), self.0.as_bytes()).cmp(&(!other.is_valid(), other.0.as_bytes()))
    }
}

impl PartialOrd for ZKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
