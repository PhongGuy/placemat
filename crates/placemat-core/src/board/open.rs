//! Opening a board file from disk within [`LIMITS`], refusing anything but a regular file.

use std::io::Read;
use std::path::Path;

use super::{LIMITS, LoadError, Loaded, load};

/// Why a board file could not be opened.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OpenError {
    /// The path is a directory, FIFO, device or anything else but a regular file.
    #[error("not a regular file")]
    NotAFile,
    /// The file is longer than [`LIMITS`]`.hard_bytes`.
    #[error("board is over the 16 MiB limit")]
    TooLarge,
    /// The file could not be read.
    #[error("cannot read the board: {0}")]
    Io(#[from] std::io::Error),
    /// The bytes are not a board the loader accepts.
    #[error(transparent)]
    Load(#[from] LoadError),
}

/// Reads and loads the board at `path`, following symlinks. Refuses a path that is not a regular
/// file, before or after opening it, and a board over [`LIMITS`]`.hard_bytes` whether that shows
/// in its metadata or only while reading; fails on I/O or load errors.
pub fn read_file(path: &Path) -> Result<Loaded, OpenError> {
    let max = u64::try_from(LIMITS.hard_bytes).unwrap_or(u64::MAX);
    // The open never blocks, so a FIFO swapped in after the stat is caught by the fstat; the
    // capped read then catches a file that grew past the limit after both checks.
    let (file, meta) = crate::fsx::open_regular(path)?.ok_or(OpenError::NotAFile)?;
    if meta.len() > max {
        return Err(OpenError::TooLarge);
    }
    Ok(load(&read_capped(file, max)?)?)
}

/// Reads all of `r`, refusing it as [`OpenError::TooLarge`] once more than `max` bytes come back.
fn read_capped(r: impl Read, max: u64) -> Result<Vec<u8>, OpenError> {
    let mut bytes = Vec::new();
    r.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(OpenError::TooLarge);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read as _};

    use super::{OpenError, read_capped};

    #[test]
    fn one_byte_over_the_cap_is_too_large() {
        let got = read_capped(std::io::repeat(b'x').take(9), 8);
        assert!(matches!(got, Err(OpenError::TooLarge)), "{got:?}");
    }

    #[test]
    fn exactly_the_cap_is_read() {
        let got = read_capped(std::io::repeat(b'x').take(8), 8).unwrap();
        assert_eq!(got.len(), 8, "bytes read");
    }

    #[test]
    fn a_short_read_is_read() {
        let got = read_capped(Cursor::new(b"abc"), 8).unwrap();
        assert_eq!(got, b"abc", "bytes read");
    }
}
