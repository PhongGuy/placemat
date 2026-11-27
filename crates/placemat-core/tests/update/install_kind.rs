//! `install_kind`, `accepts` and `staging_dir`: pure tables, every OS judged on this host.

use placemat_core::update::{
    Format, InstallError, InstallKind as K, Os, Reason as R, install_kind, staging_dir,
};
use std::path::{Path, PathBuf};

const MAC_APP: &str = "/Applications/Placemat.app/Contents/MacOS/placemat";
const MOUNT: &str = "/tmp/.mount_PlAbc/usr/bin/placemat";
const IMAGE: Option<&str> = Some("/home/u/Apps/Placemat.AppImage");

#[test]
fn install_kind_table() {
    use Os::{Linux, Macos, Windows};
    let p = PathBuf::from;
    let rows: Vec<(Os, bool, &str, Option<&str>, K)> = vec![
        // Not a release build: never owned, whatever the layout.
        (Linux, false, MOUNT, IMAGE, K::NotOwned(R::SourceBuild)),
        (
            Linux,
            false,
            "/usr/bin/placemat",
            None,
            K::NotOwned(R::SourceBuild),
        ),
        (Macos, false, MAC_APP, None, K::NotOwned(R::SourceBuild)),
        (
            Windows,
            false,
            r"C:\Program Files\Placemat\placemat.exe",
            None,
            K::NotOwned(R::SourceBuild),
        ),
        // Not absolute, or holding a `.` or `..` segment.
        (Linux, true, "placemat", IMAGE, K::NotOwned(R::Unrecognised)),
        (
            Linux,
            true,
            "/tmp/./.mount_Pl/usr/bin/placemat",
            IMAGE,
            K::NotOwned(R::Unrecognised),
        ),
        (
            Macos,
            true,
            "/Applications/../Volumes/X/Placemat.app/Contents/MacOS/placemat",
            None,
            K::NotOwned(R::Unrecognised),
        ),
        (
            Windows,
            true,
            "placemat.exe",
            None,
            K::NotOwned(R::Unrecognised),
        ),
        (
            Windows,
            true,
            r"C:\Placemat\..\placemat.exe",
            None,
            K::NotOwned(R::Unrecognised),
        ),
        // The .deb, under /usr but not /usr/local, even with $APPIMAGE set.
        (
            Linux,
            true,
            "/usr/bin/placemat",
            None,
            K::NotOwned(R::SystemPackage),
        ),
        (
            Linux,
            true,
            "//usr/bin/placemat",
            None,
            K::NotOwned(R::SystemPackage),
        ),
        (
            Linux,
            true,
            "/usr/bin/placemat",
            IMAGE,
            K::NotOwned(R::SystemPackage),
        ),
        (
            Linux,
            true,
            "/usr/local/bin/placemat",
            None,
            K::NotOwned(R::Unrecognised),
        ),
        // A mounted dmg and a translocated copy win over their bundle.
        (
            Macos,
            true,
            "/Volumes/Placemat/Placemat.app/Contents/MacOS/placemat",
            None,
            K::NotOwned(R::ReadOnlyVolume),
        ),
        (
            Macos,
            true,
            "/private/var/folders/ab/T/AppTranslocation/1234/d/Placemat.app/Contents/MacOS/placemat",
            None,
            K::NotOwned(R::Translocated),
        ),
        // The owned layouts.
        (
            Linux,
            true,
            MOUNT,
            IMAGE,
            K::AppImage(p("/home/u/Apps/Placemat.AppImage")),
        ),
        (
            Macos,
            true,
            MAC_APP,
            None,
            K::MacApp(p("/Applications/Placemat.app")),
        ),
        (
            Macos,
            true,
            MAC_APP,
            IMAGE,
            K::MacApp(p("/Applications/Placemat.app")),
        ),
        (
            Windows,
            true,
            r"C:\Users\u\AppData\Local\Placemat\placemat.exe",
            None,
            K::WindowsNsis,
        ),
        (
            Windows,
            true,
            r"\\server\share\Placemat\placemat.exe",
            None,
            K::WindowsNsis,
        ),
        (
            Windows,
            true,
            "C:/Placemat/placemat.exe",
            None,
            K::WindowsNsis,
        ),
        // Anything else.
        (
            Linux,
            true,
            "/home/u/bin/placemat",
            None,
            K::NotOwned(R::Unrecognised),
        ),
        (
            Macos,
            true,
            "/Users/u/bin/placemat",
            None,
            K::NotOwned(R::Unrecognised),
        ),
        (
            Macos,
            true,
            "/Applications/Placemat.app/placemat",
            None,
            K::NotOwned(R::Unrecognised),
        ),
    ];
    for (os, release, exe, appimage, want) in rows {
        let got = install_kind(os, release, Path::new(exe), appimage.map(Path::new));
        assert_eq!(got, want, "{os:?} release={release} {exe} {appimage:?}");
    }
}

#[test]
fn accepts_table() {
    let kinds = [
        (
            K::AppImage("/a/Placemat.AppImage".into()),
            Some(Format::AppImage),
        ),
        (
            K::MacApp("/Applications/Placemat.app".into()),
            Some(Format::App),
        ),
        (K::WindowsNsis, Some(Format::Nsis)),
        (K::NotOwned(R::SourceBuild), None),
    ];
    for (kind, fit) in kinds {
        for format in [Format::AppImage, Format::App, Format::Nsis] {
            assert_eq!(
                kind.accepts(format),
                fit == Some(format),
                "{kind:?} {format:?}"
            );
        }
    }
}

#[test]
fn staging_dir_is_beside_the_install() {
    assert_eq!(
        staging_dir(&K::AppImage("/opt/apps/Placemat.AppImage".into())),
        Ok(PathBuf::from("/opt/apps"))
    );
    assert_eq!(
        staging_dir(&K::MacApp("/Applications/Placemat.app".into())),
        Ok(PathBuf::from("/Applications"))
    );
    assert_eq!(staging_dir(&K::WindowsNsis), Ok(std::env::temp_dir()));
    assert_eq!(
        staging_dir(&K::AppImage("Placemat.AppImage".into())),
        Err(InstallError::NotOwned(R::Unrecognised))
    );
    for r in [
        R::SourceBuild,
        R::SystemPackage,
        R::ReadOnlyVolume,
        R::Translocated,
        R::Unrecognised,
    ] {
        assert_eq!(
            staging_dir(&K::NotOwned(r)),
            Err(InstallError::NotOwned(r)),
            "{r:?}"
        );
    }
}

#[test]
fn reasons_name_what_the_user_does() {
    assert_eq!(
        R::SystemPackage.to_string(),
        "this copy was installed by your package manager; download the new .deb"
    );
    assert_eq!(
        R::ReadOnlyVolume.to_string(),
        "this copy runs from the disk image; move Placemat to Applications first"
    );
    assert_eq!(
        R::Translocated.to_string(),
        "macOS runs this copy from a temporary location; move Placemat to Applications first"
    );
}
