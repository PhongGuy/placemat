//! What the updater does at launch before any check: which install this is, and the sweep of
//! leftovers beside it.

use placemat_app::updates::{local_kind, sweep_at_launch};
use placemat_core::update::{InstallKind, Reason};

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
