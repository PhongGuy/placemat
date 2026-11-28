//! `download`: the staged name, the size and hash rules, the private `.part`, and cleanup.

use crate::support::{Reply, Server, leftovers, offer, offer_within};
use placemat_core::update::{Claim, DownloadError, Format, download, sweep_staged};
use std::time::Duration;

const BODY: &[u8] = b"new build!";

/// The 99.0.0 AppImage name `claim` stages under.
fn staged(claim: &Claim) -> String {
    format!("placemat-update-{}-99.0.0.AppImage", claim.nonce())
}

fn setup() -> (Server, tempfile::TempDir, Claim) {
    let dir = tempfile::tempdir().unwrap();
    let claim = Claim::new(dir.path()).unwrap();
    (Server::start(), dir, claim)
}

#[test]
fn download_stages_a_verified_file() {
    let (s, dir, claim) = setup();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let a = offer(&s, "appimage", BODY);
    let mut seen = Vec::new();
    let st = download(&a, &claim, |got, total| seen.push((got, total))).unwrap();
    assert_eq!(std::fs::read(st.path()).unwrap(), BODY);
    assert_eq!(st.path(), dir.path().join(staged(&claim)));
    assert_eq!(st.version(), "99.0.0");
    assert_eq!(st.format(), Format::AppImage);
    assert_eq!((st.sha256(), st.size()), (a.sha256(), 10));
    assert_eq!(seen.last(), Some(&(10, 10)));
    assert_eq!(leftovers(dir.path()), [staged(&claim)]);
}

#[test]
fn download_names_each_format() {
    for (format, suffix) in [
        ("appimage", ".AppImage"),
        ("app", ".app.tar.gz"),
        ("nsis", ".exe"),
    ] {
        let (s, dir, claim) = setup();
        s.route("/artifact", Reply::Body(BODY.to_vec()));
        let st = download(&offer(&s, format, BODY), &claim, |_, _| {}).unwrap();
        let name = format!("placemat-update-{}-99.0.0{suffix}", claim.nonce());
        assert_eq!(st.path(), dir.path().join(name), "{format}");
    }
}

#[test]
fn download_refuses_long_body() {
    let (s, dir, claim) = setup();
    s.route("/artifact", Reply::Body(vec![b'x'; 1024 * 1024]));
    let mut seen = Vec::new();
    let got = download(&offer(&s, "appimage", BODY), &claim, |n, _| seen.push(n));
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
    let (s, dir, claim) = setup();
    s.route("/artifact", Reply::Body(b"new ".to_vec()));
    let got = download(&offer(&s, "appimage", BODY), &claim, |_, _| {});
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
    let (s, dir, claim) = setup();
    s.route("/artifact", Reply::Body(b"bad build!".to_vec()));
    let got = download(&offer(&s, "appimage", BODY), &claim, |_, _| {});
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
    let (s, dir, claim) = setup();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel");
    std::fs::write(&sentinel, b"keep me").unwrap();
    std::os::unix::fs::symlink(
        &sentinel,
        dir.path().join(format!("{}.part", staged(&claim))),
    )
    .unwrap();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let st = download(&offer(&s, "appimage", BODY), &claim, |_, _| {}).unwrap();
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep me");
    let meta = std::fs::symlink_metadata(st.path()).unwrap();
    assert!(meta.file_type().is_file(), "{meta:?}");
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert_eq!(leftovers(dir.path()), [staged(&claim)]);
}

#[test]
fn download_replaces_a_stale_part() {
    let (s, dir, claim) = setup();
    std::fs::write(
        dir.path().join(format!("{}.part", staged(&claim))),
        b"junk from a crash",
    )
    .unwrap();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let st = download(&offer(&s, "appimage", BODY), &claim, |_, _| {}).unwrap();
    assert_eq!(std::fs::read(st.path()).unwrap(), BODY);
    assert_eq!(leftovers(dir.path()), [staged(&claim)]);
}

#[test]
fn download_removes_part_on_failure() {
    let cases = [
        (Reply::Hangup, None),
        (Reply::Status(404), Some(DownloadError::ArtifactMissing)),
        (Reply::Status(503), Some(DownloadError::Http(503))),
    ];
    for (reply, want) in cases {
        let (s, dir, claim) = setup();
        s.route("/artifact", reply);
        let got = download(&offer(&s, "appimage", BODY), &claim, |_, _| {});
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
fn download_refuses_before_request_when_the_part_cannot_be_created() {
    let (s, dir, claim) = setup();
    // A non-empty directory at the `.part` name, which no file create or remove gets past.
    let planted = dir.path().join(format!("{}.part", staged(&claim)));
    std::fs::create_dir(&planted).unwrap();
    std::fs::write(planted.join("inside"), b"x").unwrap();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    assert_eq!(
        download(&offer(&s, "appimage", BODY), &claim, |_, _| {}).unwrap_err(),
        DownloadError::NotWritable(dir.path().to_path_buf())
    );
    assert!(
        s.seen().iter().all(|r| r.path != "/artifact"),
        "the artifact was requested"
    );
    assert!(
        planted.join("inside").is_file(),
        "the planted directory went"
    );
}

// The claim's directory vanishes before the download, so the `.part` cannot be created.
// Unix only: Windows cannot remove a directory while the claim's lock handle is open.
#[cfg(unix)]
#[test]
fn download_refuses_before_request_when_the_dir_vanished() {
    let s = Server::start();
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("gone");
    std::fs::create_dir(&gone).unwrap();
    let claim = Claim::new(&gone).unwrap();
    std::fs::remove_dir_all(&gone).unwrap();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    assert_eq!(
        download(&offer(&s, "appimage", BODY), &claim, |_, _| {}).unwrap_err(),
        DownloadError::NotWritable(gone)
    );
    assert!(
        s.seen().iter().all(|r| r.path != "/artifact"),
        "the artifact was requested"
    );
}

#[test]
fn download_times_out() {
    let (s, dir, claim) = setup();
    s.route(
        "/artifact",
        Reply::Slow(Duration::from_secs(5), BODY.to_vec()),
    );
    let a = offer_within(&s, "appimage", BODY, Duration::from_millis(200));
    assert_eq!(
        download(&a, &claim, |_, _| {}).unwrap_err(),
        DownloadError::Timeout
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
}

#[test]
fn two_claims_in_one_dir_stage_apart() {
    let (s, dir, a) = setup();
    let b = Claim::new(dir.path()).unwrap();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let listed = offer(&s, "appimage", BODY);
    let first = download(&listed, &a, |_, _| {}).unwrap();
    let second = download(&listed, &b, |_, _| {}).unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(std::fs::read(first.path()).unwrap(), BODY);
    assert_eq!(std::fs::read(second.path()).unwrap(), BODY);
    let mut want = vec![staged(&a), staged(&b)];
    want.sort();
    assert_eq!(leftovers(dir.path()), want);
}

#[test]
fn sweep_keeps_a_live_download() {
    let (s, dir, claim) = setup();
    s.route("/artifact", Reply::Body(BODY.to_vec()));
    let st = download(&offer(&s, "appimage", BODY), &claim, |_, _| {}).unwrap();
    sweep_staged(dir.path());
    assert_eq!(std::fs::read(st.path()).unwrap(), BODY);
}
