//! The banner text for a board that failed to open, was repaired, or is read-only, and the
//! tickets that let only the latest open land. Every text passes [`clean`], because loader
//! messages and file names echo bytes from the file system.

use std::path::Path;

use placemat_core::board::{End, LIMITS, OpenError, Warning};

/// The most chars one banner line shows, the closing ellipsis included.
pub const NOTICE_CHARS: usize = 240;

/// How many repairs the warnings banner lists before it counts the rest.
pub const WARNING_LINES: usize = 5;

/// The bidi format characters that can reorder text around them; std has no bidi-class query.
const BIDI_FORMAT: [char; 12] = [
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

/// Turns control characters into spaces, drops bidi format characters, and cuts the result to
/// [`NOTICE_CHARS`] chars ending in an ellipsis when it is longer.
pub fn clean(s: &str) -> String {
    // The wider invisible-character policy (zero-width and line separators) is left to later work.
    let filtered: Vec<char> = s
        .chars()
        .filter(|c| !BIDI_FORMAT.contains(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if filtered.len() <= NOTICE_CHARS {
        return filtered.into_iter().collect();
    }
    let mut cut: String = filtered.into_iter().take(NOTICE_CHARS - 1).collect();
    cut.push('\u{2026}');
    cut
}

/// "Could not open <file name>: <error>", cleaned; the file name falls back to the whole path.
pub fn error_text(path: &Path, e: &OpenError) -> String {
    let name = path.file_name().unwrap_or(path.as_os_str());
    clean(&format!("Could not open {}: {e}", name.to_string_lossy()))
}

/// One cleaned line per warning, the first [`WARNING_LINES`], then "and N more"; `None` when
/// there are no warnings.
pub fn warnings_text(warnings: &[Warning]) -> Option<String> {
    if warnings.is_empty() {
        return None;
    }
    let mut lines: Vec<String> = warnings
        .iter()
        .take(WARNING_LINES)
        .map(|w| clean(&warning_line(w)))
        .collect();
    if let Some(rest) = warnings.len().checked_sub(WARNING_LINES).filter(|n| *n > 0) {
        lines.push(format!("and {rest} more"));
    }
    Some(lines.join("\n"))
}

/// The sentence for one repair, before cleaning.
fn warning_line(w: &Warning) -> String {
    match w {
        Warning::DuplicateId { original, renamed } => {
            format!("Repeated id {original} renamed to {renamed}")
        }
        Warning::DanglingBinding { arrow, end, target } => {
            let side = match end {
                End::From => "start",
                End::To | _ => "end",
            };
            format!("Arrow {arrow}: its {side} was bound to missing {target} and is now free")
        }
        Warning::SeedClamped { id } => {
            format!("The seed of {id} was out of range and was clamped")
        }
        Warning::InvalidZ { id } => format!("Malformed z key on {id} replaced; it paints last"),
        Warning::OverTeamLimit { bytes } => format!(
            "The file is {} KiB, over the {} KiB a team shares comfortably",
            bytes.div_ceil(1024),
            LIMITS.team_bytes / 1024
        ),
        _ => "The board was repaired while loading".to_owned(),
    }
}

/// The read-only banner for a board whose file declares the newer version `found`.
pub fn readonly_text(found: u32) -> String {
    format!("This board was made by a newer Placemat (version {found}), so it opened read-only.")
}

/// One open's place in the order opens were started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ticket(u64);

/// Issues open tickets so a result can tell whether a later open has superseded it.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Tickets {
    latest: u64,
}

impl Tickets {
    /// Starts a new open, superseding every ticket issued before it.
    pub fn issue(&mut self) -> Ticket {
        self.latest = self.latest.wrapping_add(1);
        Ticket(self.latest)
    }

    /// Whether `t` belongs to the most recently started open.
    pub fn is_latest(&self, t: Ticket) -> bool {
        t.0 == self.latest
    }
}
