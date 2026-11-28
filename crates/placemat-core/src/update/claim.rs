//! Ownership of staged updates: each process claims the staging directory under a random
//! nonce its staged files carry, and the sweep removes only what a gone process left.

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub(super) const PREFIX: &str = "placemat-update-";
const LOCK_SUFFIX: &str = ".lock";
const NONCE_LEN: usize = 16;
// An unlocked lock file younger than this may belong to a claim still being taken.
const LOCK_GRACE: Duration = Duration::from_secs(60);

/// A process's hold on a staging directory: a random nonce its staged files carry, and an
/// OS lock on `placemat-update-<nonce>.lock` held until the claim drops.
#[non_exhaustive]
#[derive(Debug)]
pub struct Claim {
    dir: PathBuf,
    nonce: String,
    _lock: File,
}

impl Claim {
    /// Creates the lock file exclusively (0600 on Unix) under a fresh nonce and locks it.
    /// Errors only when the file cannot be created; a lock the filesystem refuses is ignored.
    pub fn new(dir: &Path) -> std::io::Result<Claim> {
        // A nonce collision is vanishingly rare; a few retries cover it without looping on
        // a directory that refuses every name for another reason.
        let mut last = None;
        for _ in 0..4 {
            let nonce = format!("{:016x}", fastrand::u64(..));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            match options.open(dir.join(lock_name(&nonce))) {
                Ok(file) => {
                    // Blocking: a sweeper holds this lock only for a moment. A filesystem
                    // without locks still downloads; its sweep then keeps the files.
                    let _ = file.lock();
                    return Ok(Claim {
                        dir: dir.to_path_buf(),
                        nonce,
                        _lock: file,
                    });
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => last = Some(e),
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| ErrorKind::AlreadyExists.into()))
    }

    /// The claimed directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 16 lowercase hex characters.
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
}

/// What a `placemat-update-` name says about its owner.
enum Owner<'a> {
    /// No nonce: a name from before claims, or one no claim made.
    None,
    /// A staged file or `.part` of the claim under this nonce.
    Data(&'a str),
    /// A claim's lock file.
    Lock,
}

fn owner(name: &str) -> Owner<'_> {
    let Some(rest) = name.strip_prefix(PREFIX) else {
        return Owner::None;
    };
    let Some((nonce, tail)) = rest.split_at_checked(NONCE_LEN) else {
        return Owner::None;
    };
    if !nonce
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Owner::None;
    }
    match tail {
        LOCK_SUFFIX => Owner::Lock,
        t if t.starts_with('-') => Owner::Data(nonce),
        _ => Owner::None,
    }
}

fn lock_name(nonce: &str) -> String {
    format!("{PREFIX}{nonce}{LOCK_SUFFIX}")
}

/// Removes the regular files and links in `dir` whose names start with `placemat-update-`
/// and that no live process owns, best effort. A name carrying a nonce is kept while its
/// lock is held or cannot be probed; an unlocked lock file goes once it is a minute old.
/// Directories and every other name are left alone.
pub fn sweep_staged(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str().filter(|n| n.starts_with(PREFIX)) else {
            continue;
        };
        // lstat, so a link is removed itself and never followed.
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !(meta.is_file() || meta.is_symlink()) {
            continue;
        }
        let lock = match owner(name) {
            Owner::None => {
                let _ = std::fs::remove_file(entry.path());
                continue;
            }
            Owner::Data(nonce) => dir.join(lock_name(nonce)),
            Owner::Lock if aged(&meta) => entry.path(),
            Owner::Lock => continue,
        };
        // The probe's lock is held across the remove, so the owner's files go only while
        // no process holds them; any probe error other than a missing lock file keeps them.
        match File::open(&lock) {
            Err(e) if e.kind() == ErrorKind::NotFound => {
                let _ = std::fs::remove_file(entry.path());
            }
            Err(_) => {}
            Ok(probe) => {
                if probe.try_lock().is_ok() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

fn aged(meta: &std::fs::Metadata) -> bool {
    meta.modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .is_some_and(|d| d > LOCK_GRACE)
}
