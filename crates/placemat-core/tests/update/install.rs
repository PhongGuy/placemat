//! `install`: the refusal order, the re-hash, the AppImage swap, the NSIS hand-off, the bundle.

use crate::support::{leftovers, stage};
use placemat_core::update::{InstallError, InstallKind, Reason, Relaunch, install};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const NEW: &[u8] = b"new build";
const BAD: &[u8] = b"bad build";
const SETUP: &str = "placemat-update-99.0.0-setup.exe";

fn executable(dir: &Path, name: &str) -> PathBuf {
    let target = dir.join(name);
    std::fs::write(&target, b"old").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    target
}

#[cfg(unix)]
fn mode(p: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(p).unwrap().permissions().mode() & 0o7777
}

#[test]
fn install_swaps_the_appimage_and_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let target = executable(dir.path(), "Placemat.AppImage");
    let st = stage(dir.path(), "appimage", NEW);
    assert_eq!(
        install(&InstallKind::AppImage(target.clone()), st),
        Ok(Relaunch::Restart(target.clone()))
    );
    assert_eq!(std::fs::read(&target).unwrap(), NEW);
    #[cfg(unix)]
    assert_eq!(mode(&target), 0o755);
    assert_eq!(leftovers(dir.path()), ["Placemat.AppImage"]);
}

#[test]
fn install_hands_off_the_nsis_installer() {
    let dir = tempfile::tempdir().unwrap();
    let st = stage(dir.path(), "nsis", NEW);
    let program = dir.path().join(SETUP);
    let args = vec![OsString::from("/P"), OsString::from("/R")];
    assert_eq!(
        install(&InstallKind::WindowsNsis, st),
        Ok(Relaunch::Installer {
            program: program.clone(),
            args
        })
    );
    assert_eq!(std::fs::read(&program).unwrap(), NEW);
    assert_eq!(leftovers(dir.path()), [SETUP]);
}

#[test]
fn install_refuses_not_owned_before_format() {
    let dir = tempfile::tempdir().unwrap();
    for reason in [
        Reason::SystemPackage,
        Reason::SourceBuild,
        Reason::Translocated,
    ] {
        let st = stage(dir.path(), "appimage", NEW);
        assert_eq!(
            install(&InstallKind::NotOwned(reason), st),
            Err(InstallError::NotOwned(reason))
        );
        assert!(leftovers(dir.path()).is_empty(), "{reason:?}");
    }
}

#[test]
fn install_refuses_wrong_format() {
    let dir = tempfile::tempdir().unwrap();
    let target = executable(dir.path(), "Placemat.AppImage");
    let st = stage(dir.path(), "app", NEW);
    assert_eq!(
        install(&InstallKind::AppImage(target.clone()), st),
        Err(InstallError::WrongFormat)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
    assert_eq!(leftovers(dir.path()), ["Placemat.AppImage"]);
    let st = stage(dir.path(), "appimage", NEW);
    assert_eq!(
        install(&InstallKind::WindowsNsis, st),
        Err(InstallError::WrongFormat)
    );
    assert_eq!(leftovers(dir.path()), ["Placemat.AppImage"]);
}

// The bytes change after the download verified them; each kind must notice before acting.
#[test]
fn install_rehashes_and_refuses_changed_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = executable(dir.path(), "Placemat.AppImage");
    let st = stage(dir.path(), "appimage", NEW);
    std::fs::write(st.path(), BAD).unwrap();
    assert_eq!(
        install(&InstallKind::AppImage(target.clone()), st),
        Err(InstallError::HashMismatch)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
    assert_eq!(leftovers(dir.path()), ["Placemat.AppImage"]);

    let dir = tempfile::tempdir().unwrap();
    let st = stage(dir.path(), "nsis", NEW);
    std::fs::write(st.path(), [NEW, b"!"].concat()).unwrap();
    assert_eq!(
        install(&InstallKind::WindowsNsis, st),
        Err(InstallError::HashMismatch)
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );

    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Placemat.app");
    std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    std::fs::write(bundle.join("Contents/MacOS/placemat"), b"old exe").unwrap();
    let st = stage(dir.path(), "app", NEW);
    std::fs::write(st.path(), BAD).unwrap();
    assert_eq!(
        install(&InstallKind::MacApp(bundle.clone()), st),
        Err(InstallError::HashMismatch)
    );
    assert_eq!(
        std::fs::read(bundle.join("Contents/MacOS/placemat")).unwrap(),
        b"old exe"
    );
    assert_eq!(leftovers(dir.path()), ["Placemat.app"]);
}

// The link points at identical bytes, so only the lstat rule can refuse it.
#[cfg(unix)]
#[test]
fn install_refuses_symlinked_stage() {
    for format in ["appimage", "nsis"] {
        let (dir, outside) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let target = executable(dir.path(), "Placemat.AppImage");
        let st = stage(dir.path(), format, NEW);
        let twin = outside.path().join("twin");
        std::fs::write(&twin, NEW).unwrap();
        std::fs::remove_file(st.path()).unwrap();
        std::os::unix::fs::symlink(&twin, st.path()).unwrap();
        let kind = match format {
            "appimage" => InstallKind::AppImage(target.clone()),
            _ => InstallKind::WindowsNsis,
        };
        let got = install(&kind, st);
        assert!(
            matches!(got, Err(InstallError::Io { .. })),
            "{format}: {got:?}"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"old", "{format}");
        assert_eq!(
            std::fs::read(&twin).unwrap(),
            NEW,
            "{format}: the link was followed"
        );
        assert_eq!(leftovers(dir.path()), ["Placemat.AppImage"], "{format}");
    }
}

#[test]
fn install_refuses_stage_elsewhere() {
    let (dir, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let target = executable(dir.path(), "Placemat.AppImage");
    let st = stage(other.path(), "appimage", NEW);
    let got = install(&InstallKind::AppImage(target.clone()), st);
    assert!(matches!(got, Err(InstallError::Io { .. })), "{got:?}");
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
    assert!(
        leftovers(other.path()).is_empty(),
        "{:?}",
        leftovers(other.path())
    );
}

#[cfg(unix)]
#[test]
fn install_masks_special_bits() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let target = executable(dir.path(), "Placemat.AppImage");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o7755)).unwrap();
    let st = stage(dir.path(), "appimage", NEW);
    install(&InstallKind::AppImage(target.clone()), st).unwrap();
    assert_eq!(mode(&target), 0o755);
}

// A non-empty directory where the AppImage belongs makes the rename fail with an I/O error.
#[cfg(unix)]
#[test]
fn install_failed_swap_keeps_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("Placemat.AppImage");
    std::fs::create_dir_all(blocked.join("inside")).unwrap();
    let st = stage(dir.path(), "appimage", NEW);
    let got = install(&InstallKind::AppImage(blocked.clone()), st);
    assert!(matches!(got, Err(InstallError::Io { .. })), "{got:?}");
    assert!(blocked.join("inside").is_dir());
    assert_eq!(leftovers(dir.path()), ["Placemat.AppImage"]);
}

#[cfg(unix)]
#[test]
fn install_into_a_read_only_directory_is_not_writable() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let target = executable(dir.path(), "Placemat.AppImage");
    let st = stage(dir.path(), "appimage", NEW);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let root = std::fs::write(dir.path().join("probe"), b"").is_ok();
    let got = (!root).then(|| install(&InstallKind::AppImage(target.clone()), st));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    if let Some(got) = got {
        assert_eq!(
            got,
            Err(InstallError::NotWritable(dir.path().to_path_buf()))
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
    }
}

#[cfg(unix)]
fn bundle_tarball(exe: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut b = tar::Builder::new(Vec::new());
    for d in [
        "Placemat.app/",
        "Placemat.app/Contents/",
        "Placemat.app/Contents/MacOS/",
    ] {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Directory);
        h.set_mode(0o755);
        h.set_size(0);
        b.append_data(&mut h, d, std::io::empty()).unwrap();
    }
    let mut h = tar::Header::new_gnu();
    h.set_mode(0o755);
    h.set_size(exe.len() as u64);
    b.append_data(&mut h, "Placemat.app/Contents/MacOS/placemat", exe)
        .unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&b.into_inner().unwrap()).unwrap();
    gz.finish().unwrap()
}

#[cfg(unix)]
#[test]
fn install_mac_app_from_its_tarball_and_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Placemat.app");
    std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    std::fs::write(bundle.join("Contents/MacOS/placemat"), b"old exe").unwrap();
    let st = stage(dir.path(), "app", &bundle_tarball(b"new exe"));
    assert_eq!(
        install(&InstallKind::MacApp(bundle.clone()), st),
        Ok(Relaunch::Restart(bundle.clone()))
    );
    assert_eq!(
        std::fs::read(bundle.join("Contents/MacOS/placemat")).unwrap(),
        b"new exe"
    );
    assert_eq!(leftovers(dir.path()), ["Placemat.app"]);
}
