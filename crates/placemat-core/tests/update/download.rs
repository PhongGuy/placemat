//! `download`: the staged name, the size and hash rules, the private `.part`, and cleanup.

use crate::support::{Reply, Server, leftovers, offer, offer_within};
use placemat_core::update::{DownloadError, Format, download};
use std::time::Duration;

const BODY: &[u8] = b"new build!";
const STAGED: &str = "placemat-update-99.0.0.AppImage";

fn setup() -> (Server, tempfile::TempDir) {
    (Server::start(), tempfile::tempdir().unwrap())
}

#[test]
fn download_stages_a_verified_file() {
    let (s, dir) = setup();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let a = offer(&s, "appimage", BODY);
    let mut seen = Vec::new();
    let staged = download(&a, dir.path(), |got, total| seen.push((got, total))).unwrap();
    assert_eq!(std::fs::read(staged.path()).unwrap(), BODY);
    assert_eq!(staged.path(), dir.path().join(STAGED));
    assert_eq!(staged.version(), "99.0.0");
    assert_eq!(staged.format(), Format::AppImage);
    assert_eq!((staged.sha256(), staged.size()), (a.sha256(), 10));
    assert_eq!(seen.last(), Some(&(10, 10)));
    assert_eq!(leftovers(dir.path()), [STAGED]);
}

#[test]
fn download_names_each_format() {
    for (format, name) in [
        ("appimage", "placemat-update-99.0.0.AppImage"),
        ("app", "placemat-update-99.0.0.app.tar.gz"),
        ("nsis", "placemat-update-99.0.0.exe"),
    ] {
        let (s, dir) = setup();
        s.route("/artifact", Reply::Body(BODY.to_vec()));
        let staged = download(&offer(&s, format, BODY), dir.path(), |_, _| {}).unwrap();
        assert_eq!(staged.path(), dir.path().join(name), "{format}");
    }
}

#[test]
fn download_refuses_long_body() {
    let (s, dir) = setup();
    s.route("/artifact", Reply::Body(vec![b'x'; 1024 * 1024]));
    let mut seen = Vec::new();
    let got = download(&offer(&s, "appimage", BODY), dir.path(), |n, _| {
        seen.push(n)
    });
    assert!(
        matches!(got, Err(DownloadError::SizeMismatch { expected: 10, .. })),
        "{got:?}"
    );
    assert!(
        seen.iter().all(|&n| n <= 10),
        "wrote past the listed size: {seen:?}"
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
}

#[test]
fn download_refuses_short_body() {
    let (s, dir) = setup();
    s.route("/artifact", Reply::Body(b"new ".to_vec()));
    let got = download(&offer(&s, "appimage", BODY), dir.path(), |_, _| {});
    assert_eq!(
        got.unwrap_err(),
        DownloadError::SizeMismatch {
            expected: 10,
            got: 4
        }
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
}

#[test]
fn download_refuses_wrong_sha() {
    let (s, dir) = setup();
    s.route("/artifact", Reply::Body(b"bad build!".to_vec()));
    let got = download(&offer(&s, "appimage", BODY), dir.path(), |_, _| {});
    assert_eq!(got.unwrap_err(), DownloadError::HashMismatch);
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
}

// A link planted at the `.part` name must never carry the download's bytes elsewhere.
#[cfg(unix)]
#[test]
fn download_part_is_0600_and_exclusive() {
    use std::os::unix::fs::PermissionsExt as _;
    let (s, dir) = setup();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel");
    std::fs::write(&sentinel, b"keep me").unwrap();
    std::os::unix::fs::symlink(&sentinel, dir.path().join(format!("{STAGED}.part"))).unwrap();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let staged = download(&offer(&s, "appimage", BODY), dir.path(), |_, _| {}).unwrap();
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep me");
    let meta = std::fs::symlink_metadata(staged.path()).unwrap();
    assert!(meta.file_type().is_file(), "{meta:?}");
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert_eq!(leftovers(dir.path()), [STAGED]);
}

#[test]
fn download_replaces_a_stale_part() {
    let (s, dir) = setup();
    std::fs::write(
        dir.path().join(format!("{STAGED}.part")),
        b"junk from a crash",
    )
    .unwrap();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let staged = download(&offer(&s, "appimage", BODY), dir.path(), |_, _| {}).unwrap();
    assert_eq!(std::fs::read(staged.path()).unwrap(), BODY);
    assert_eq!(leftovers(dir.path()), [STAGED]);
}

#[test]
fn download_removes_part_on_failure() {
    let cases = [
        (Reply::Hangup, None),
        (Reply::Status(404), Some(DownloadError::ArtifactMissing)),
        (Reply::Status(503), Some(DownloadError::Http(503))),
    ];
    for (reply, want) in cases {
        let (s, dir) = setup();
        s.route("/artifact", reply);
        let got = download(&offer(&s, "appimage", BODY), dir.path(), |_, _| {});
        match want {
            Some(want) => assert_eq!(got.unwrap_err(), want),
            None => assert!(matches!(got, Err(DownloadError::Offline(_))), "{got:?}"),
        }
        assert!(
            leftovers(dir.path()).is_empty(),
            "{:?}",
            leftovers(dir.path())
        );
    }
}

#[test]
fn download_refuses_unwritable_dir_before_request() {
    let (s, dir) = setup();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let a = offer(&s, "appimage", BODY);
    let missing = dir.path().join("missing");
    assert_eq!(
        download(&a, &missing, |_, _| {}).unwrap_err(),
        DownloadError::NotWritable(missing)
    );
    assert!(
        s.seen().iter().all(|r| r.path != "/artifact"),
        "the artifact was requested"
    );
}

#[test]
fn download_times_out() {
    let (s, dir) = setup();
    s.route(
        "/artifact",
        Reply::Slow(Duration::from_secs(5), BODY.to_vec()),
    );
    let a = offer_within(&s, "appimage", BODY, Duration::from_millis(200));
    assert_eq!(
        download(&a, dir.path(), |_, _| {}).unwrap_err(),
        DownloadError::Timeout
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
}

#[cfg(unix)]
#[test]
fn sweep_removes_only_staged_files_and_links() {
    use placemat_core::update::sweep_staged;
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
        leftovers(dir.path()),
        ["Placemat.AppImage", "notes.txt", "placemat-update-dir"]
    );
    assert_eq!(std::fs::read(&kept).unwrap(), b"keep");
}
