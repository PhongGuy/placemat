//! Opening a board file from disk within [`LIMITS`], refusing anything but a regular file.

use std::io::Read as _;
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
/// file or is over [`LIMITS`]`.hard_bytes` before reading it, and fails on I/O or load errors.
pub fn read_file(path: &Path) -> Result<Loaded, OpenError> {
    let max = u64::try_from(LIMITS.hard_bytes).unwrap_or(u64::MAX);
    // Refuse anything but a regular file within the cap before opening, so a FIFO or device is
    // never opened; then recheck the handle and bound the read in case the file changed between.
    let check = |meta: &std::fs::Metadata| {
        if !meta.is_file() {
            Err(OpenError::NotAFile)
        } else if meta.len() > max {
            Err(OpenError::TooLarge)
        } else {
            Ok(())
        }
    };
    check(&std::fs::metadata(path)?)?;
    let file = std::fs::File::open(path)?;
    check(&file.metadata()?)?;
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    Ok(load(&bytes)?)
}
