//! `Claim` and `sweep_staged`: owner-tagged names, the lock that keeps a live copy's files,
//! and the sweep of what a gone copy left.

use placemat_core::update::{Claim, sweep_staged};
use std::fs::{File, TryLockError};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Every name in `dir`, lock files included, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn lock_name(claim: &Claim) -> String {
    format!("placemat-update-{}.lock", claim.nonce())
}

fn data_names(nonce: &str) -> [String; 2] {
    let staged = format!("placemat-update-{nonce}-99.0.0.AppImage");
    [staged.clone(), format!("{staged}.part")]
}

fn write_all(dir: &Path, names: &[String]) {
    for name in names {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
}

fn age(path: &Path, by: Duration) {
    let f = File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - by).unwrap();
}

#[test]
fn claim_holds_its_lock_until_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let claim = Claim::new(dir.path()).unwrap();
    assert_eq!(claim.dir(), dir.path());
    assert_eq!(claim.nonce().len(), 16, "{}", claim.nonce());
    assert!(
        claim
            .nonce()
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{}",
        claim.nonce()
    );
    let lock = dir.path().join(lock_name(&claim));
    assert_eq!(names(dir.path()), [lock_name(&claim)]);
    let other = File::open(&lock).unwrap();
    assert!(
        matches!(other.try_lock(), Err(TryLockError::WouldBlock)),
        "a second handle took a live claim's lock"
    );
    drop(claim);
    assert!(other.try_lock().is_ok(), "the lock outlived its claim");
}

#[test]
fn claims_in_one_dir_get_distinct_nonces() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (
        Claim::new(dir.path()).unwrap(),
        Claim::new(dir.path()).unwrap(),
    );
    assert_ne!(a.nonce(), b.nonce());
    assert_eq!(names(dir.path()).len(), 2, "{:?}", names(dir.path()));
}

#[test]
fn claim_refuses_a_missing_dir() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Claim::new(&dir.path().join("missing")).is_err());
}

#[test]
fn sweep_keeps_a_live_claims_files() {
    let dir = tempfile::tempdir().unwrap();
    let claim = Claim::new(dir.path()).unwrap();
    let data = data_names(claim.nonce());
    write_all(dir.path(), &data);
    sweep_staged(dir.path());
    let mut want = vec![lock_name(&claim), data[0].clone(), data[1].clone()];
    want.sort();
    assert_eq!(names(dir.path()), want);
}

#[test]
fn sweep_keeps_a_live_claim_however_old() {
    let dir = tempfile::tempdir().unwrap();
    let claim = Claim::new(dir.path()).unwrap();
    let data = data_names(claim.nonce());
    write_all(dir.path(), &data);
    age(
        &dir.path().join(lock_name(&claim)),
        Duration::from_secs(3600),
    );
    sweep_staged(dir.path());
    assert_eq!(names(dir.path()).len(), 3, "{:?}", names(dir.path()));
}

#[test]
fn sweep_removes_a_gone_claims_files_but_keeps_its_fresh_lock() {
    let dir = tempfile::tempdir().unwrap();
    let claim = Claim::new(dir.path()).unwrap();
    write_all(dir.path(), &data_names(claim.nonce()));
    let lock = lock_name(&claim);
    drop(claim);
    sweep_staged(dir.path());
    assert_eq!(names(dir.path()), [lock]);
}

#[test]
fn sweep_removes_an_aged_unlocked_lock() {
    let dir = tempfile::tempdir().unwrap();
    let claim = Claim::new(dir.path()).unwrap();
    let lock = dir.path().join(lock_name(&claim));
    drop(claim);
    age(&lock, Duration::from_secs(120));
    sweep_staged(dir.path());
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[test]
fn sweep_removes_owned_names_with_no_lock_file_and_unparseable_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut all = data_names("0123456789abcdef").to_vec();
    all.extend([
        "placemat-update-0123456789ABCDEF-99.0.0.AppImage".to_owned(),
        "placemat-update-0123-99.0.0.AppImage".to_owned(),
        "placemat-update-99.0.0-setup.exe".to_owned(),
    ]);
    write_all(dir.path(), &all);
    sweep_staged(dir.path());
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[cfg(unix)]
#[test]
fn sweep_removes_only_staged_files_and_links() {
    let (dir, outside) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let kept = outside.path().join("kept");
    std::fs::write(&kept, b"keep").unwrap();
    for name in [
        "placemat-update-98.0.0.AppImage",
        "placemat-update-98.0.0.AppImage.part",
        "Placemat.AppImage",
        "notes.txt",
    ] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    std::os::unix::fs::symlink(&kept, dir.path().join("placemat-update-link")).unwrap();
    std::fs::create_dir(dir.path().join("placemat-update-dir")).unwrap();
    sweep_staged(dir.path());
    assert_eq!(
        names(dir.path()),
        ["Placemat.AppImage", "notes.txt", "placemat-update-dir"]
    );
    assert_eq!(std::fs::read(&kept).unwrap(), b"keep");
}
