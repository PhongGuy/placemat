//! What the updater does at launch before any check: which install this is, and the sweep of
//! leftovers beside it.

use placemat_app::updates::{local_kind, staging_claim, sweep_at_launch};
use placemat_core::update::{InstallKind, Reason};
use std::sync::Arc;

#[test]
fn a_test_build_is_a_source_build() {
    assert_eq!(
        local_kind(),
        InstallKind::NotOwned(Reason::SourceBuild),
        "tests never build as a release"
    );
}

#[test]
fn launch_sweeps_leftovers_beside_the_install() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "Placemat.AppImage",
        "notes.txt",
        "placemat-update-1.2.0.AppImage.part",
        "placemat-update-1.1.0.AppImage",
    ] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    sweep_at_launch(&InstallKind::AppImage(dir.path().join("Placemat.AppImage")));
    let mut left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, ["Placemat.AppImage", "notes.txt"]);
}

fn lock_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_str()
                .is_some_and(|n| n.ends_with(".lock"))
        })
        .count()
}

#[test]
fn launch_sweep_takes_no_claim() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Placemat.AppImage"), b"x").unwrap();
    sweep_at_launch(&InstallKind::AppImage(dir.path().join("Placemat.AppImage")));
    assert_eq!(lock_files(dir.path()), 0, "the launch left a lock file");
}

#[test]
fn installs_in_one_process_share_one_claim() {
    let dir = tempfile::tempdir().unwrap();
    let kind = InstallKind::AppImage(dir.path().join("Placemat.AppImage"));
    let mut slot = None;
    let first = staging_claim(&mut slot, &kind).unwrap();
    let second = staging_claim(&mut slot, &kind).unwrap();
    assert!(
        Arc::ptr_eq(&first, &second),
        "a later Install took a new claim"
    );
    assert_eq!(first.dir(), dir.path());
    assert_eq!(lock_files(dir.path()), 1);
}

#[test]
fn a_failed_claim_leaves_no_claim_and_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    let kind = InstallKind::AppImage(missing.join("Placemat.AppImage"));
    let mut slot = None;
    assert!(
        staging_claim(&mut slot, &kind).is_err(),
        "claimed a missing directory"
    );
    assert!(slot.is_none(), "a failed claim was kept");
    assert!(!missing.exists(), "the claim created the directory");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "a file was left"
    );
}

#[test]
fn an_install_the_updater_does_not_own_takes_no_claim() {
    let mut slot = None;
    let kind = InstallKind::NotOwned(Reason::SourceBuild);
    assert!(
        staging_claim(&mut slot, &kind).is_err(),
        "claimed for a source build"
    );
    assert!(slot.is_none(), "a failed claim was kept");
}
