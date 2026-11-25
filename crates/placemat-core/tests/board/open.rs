//! read_file: the file-type and size checks before any read, then the loader.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use placemat_core::board::{LIMITS, LoadError, OpenError, Warning, load, read_file};

use crate::util::fixture;

fn canonical(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/boards/canonical/{name}.board"))
}

/// A valid board padded with newlines to exactly `len` bytes.
fn padded(len: usize) -> Vec<u8> {
    let mut bytes = b"placemat-board version=0\n".to_vec();
    bytes.resize(len, b'\n');
    bytes
}

#[test]
fn a_board_file_loads_like_its_bytes() {
    let loaded = read_file(&canonical("rect")).unwrap();
    assert_eq!(
        loaded,
        load(fixture("canonical/rect.board").as_bytes()).unwrap()
    );
}

#[test]
fn a_missing_path_is_an_io_error_and_a_directory_is_not_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let missing = read_file(&dir.path().join("nope.board"));
    assert!(
        matches!(&missing, Err(OpenError::Io(e)) if e.kind() == ErrorKind::NotFound),
        "{missing:?}"
    );
    assert!(matches!(read_file(dir.path()), Err(OpenError::NotAFile)));
}

#[test]
fn exactly_hard_bytes_loads_and_one_more_is_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("big.board");
    std::fs::write(&at, padded(LIMITS.hard_bytes)).unwrap();
    let loaded = read_file(&at).unwrap();
    assert_eq!(
        loaded.warnings,
        [Warning::OverTeamLimit {
            bytes: LIMITS.hard_bytes
        }]
    );
    std::fs::write(&at, padded(LIMITS.hard_bytes + 1)).unwrap();
    assert!(matches!(read_file(&at), Err(OpenError::TooLarge)));
}

#[test]
fn bytes_that_are_not_a_board_are_a_load_error() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("junk.board");
    std::fs::write(&at, [0xff, 0xfe]).unwrap();
    assert!(matches!(
        read_file(&at),
        Err(OpenError::Load(LoadError::NotUtf8))
    ));
}

#[cfg(unix)]
mod unix {
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    use placemat_core::board::{Loaded, OpenError, read_file};

    use super::canonical;

    /// Reads on another thread, so a wrongful open of a FIFO fails the test instead of hanging it.
    fn read_or_hang(path: PathBuf) -> Result<Loaded, OpenError> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || drop(tx.send(read_file(&path))));
        rx.recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("read_file blocked"))
    }

    #[test]
    fn a_fifo_a_symlink_to_it_and_a_device_are_refused_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe.board");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success(), "mkfifo failed");
        let link = dir.path().join("link.board");
        std::os::unix::fs::symlink(&fifo, &link).unwrap();
        for path in [fifo, link, PathBuf::from("/dev/null")] {
            let got = read_or_hang(path.clone());
            assert!(
                matches!(got, Err(OpenError::NotAFile)),
                "{}: {got:?}",
                path.display()
            );
        }
    }

    #[test]
    fn a_symlink_to_a_board_is_followed() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link.board");
        std::os::unix::fs::symlink(canonical("rect"), &link).unwrap();
        assert!(read_or_hang(link).is_ok(), "the link was not followed");
    }
}
