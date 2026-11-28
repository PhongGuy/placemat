//! Filesystem helpers shared by the board reader and the updater.

use std::fs::{File, Metadata};
use std::io;
use std::path::Path;

/// Opens `path` for reading, following symlinks, only if it is a regular file both before and
/// after the open. Returns the handle with its own metadata, `None` for anything else, and keeps
/// the [`io::ErrorKind`] of any I/O error. Size checks are left to the caller.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the board reader adopts it next")
)]
pub(crate) fn open_regular(path: &Path) -> io::Result<Option<(File, Metadata)>> {
    if !std::fs::metadata(path)?.is_file() {
        return Ok(None);
    }
    let file = open_nonblocking(path)?;
    let meta = file.metadata()?;
    Ok(meta.is_file().then_some((file, meta)))
}

/// Opens `path` read-only without blocking, so a FIFO swapped in after a stat cannot hang the
/// caller. `O_NONBLOCK` stays set on the handle; it has no effect on regular-file reads.
#[cfg(unix)]
fn open_nonblocking(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
}

/// Opens `path` read-only; elsewhere than Unix there are no FIFOs to block on.
#[cfg(not(unix))]
fn open_nonblocking(path: &Path) -> io::Result<File> {
    File::open(path)
}

// Two attributes, not `all(test, unix)`: clippy's allow-unwrap-in-tests matches only `cfg(test)`.
#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{open_nonblocking, open_regular};

    fn mkfifo(dir: &Path) -> PathBuf {
        let fifo = dir.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success(), "mkfifo failed");
        fifo
    }

    #[test]
    fn opening_a_fifo_with_no_writer_returns_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = mkfifo(dir.path());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let is_file = open_nonblocking(&fifo)
                .and_then(|f| f.metadata())
                .map(|m| m.is_file());
            drop(tx.send(is_file));
        });
        let got = rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("open blocked on a FIFO"));
        assert!(!got.unwrap(), "fstat called a FIFO a regular file");
    }

    #[test]
    fn a_fifo_is_not_regular() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = mkfifo(dir.path());
        assert!(open_regular(&fifo).unwrap().is_none(), "FIFO accepted");
    }

    #[test]
    fn a_directory_is_not_regular() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            open_regular(dir.path()).unwrap().is_none(),
            "directory accepted"
        );
    }

    #[test]
    fn a_regular_file_opens_with_its_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"abc").unwrap();
        let (_, meta) = open_regular(&path).unwrap().unwrap();
        assert_eq!(meta.len(), 3, "fstat length");
    }

    #[test]
    fn a_missing_path_keeps_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = open_regular(&dir.path().join("missing")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound, "error kind");
    }
}
