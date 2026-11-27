//! The update notice's states over the plain model the app keeps in a global: what a check
//! result shows, what Install downloads, and what stays armed for quit.

use std::path::PathBuf;

use placemat_app::updates::{Notice, Offer, UpdateModel};
use placemat_core::update::{CheckError, Format, InstallKind, Reason};

type Model = UpdateModel<&'static str, &'static str>;

fn appimage() -> InstallKind {
    InstallKind::AppImage(PathBuf::from("/opt/Placemat.AppImage"))
}

/// A successful check offering `version` as `format`; the payload is the version itself.
fn found(version: &'static str, format: Format) -> Result<Option<Offer<&'static str>>, CheckError> {
    Ok(Some(Offer::new(version, format, version)))
}

#[test]
fn install_offered_only_when_kind_accepts_format() {
    let mut m = Model::new(appimage());
    assert_eq!(m.notice(), None, "nothing before a check");
    m.checked(found("1.2.0", Format::AppImage));
    assert_eq!(m.notice(), Some(Notice::Offered("1.2.0")));
    assert_eq!(
        m.start_download(),
        Some(&"1.2.0"),
        "Install downloads the offer"
    );
    assert_eq!(m.notice(), Some(Notice::Downloading("1.2.0")));
    assert_eq!(m.start_download(), None, "one download at a time");
    for (kind, format) in [
        (
            InstallKind::MacApp(PathBuf::from("/Applications/Placemat.app")),
            Format::App,
        ),
        (InstallKind::WindowsNsis, Format::Nsis),
    ] {
        let mut m = Model::new(kind);
        m.checked(found("1.2.0", format));
        assert_eq!(m.notice(), Some(Notice::Offered("1.2.0")), "{format:?}");
    }
}

#[test]
fn notowned_and_wrong_format_give_howto() {
    for reason in [
        Reason::SourceBuild,
        Reason::SystemPackage,
        Reason::ReadOnlyVolume,
        Reason::Translocated,
        Reason::Unrecognised,
    ] {
        let mut m = Model::new(InstallKind::NotOwned(reason));
        m.checked(found("1.2.0", Format::AppImage));
        let want = Notice::HowTo {
            version: "1.2.0",
            reason: Some(reason),
        };
        assert_eq!(m.notice(), Some(want));
        assert_eq!(m.start_download(), None, "{reason:?} must never download");
        assert!(m.take_armed().is_none(), "{reason:?} armed an install");
    }
    let mut m = Model::new(appimage());
    m.checked(found("1.2.0", Format::App));
    assert_eq!(
        m.notice(),
        Some(Notice::HowTo {
            version: "1.2.0",
            reason: None
        })
    );
    assert_eq!(
        m.start_download(),
        None,
        "a refused format must never download"
    );
}

#[test]
fn later_hides_for_session_and_ticks_stay_quiet() {
    let mut m = Model::new(appimage());
    m.checked(found("1.2.0", Format::AppImage));
    m.later();
    assert_eq!(m.notice(), None);
    assert!(!m.should_check(), "a dismissed notice needs no more checks");
    m.checked(found("1.3.0", Format::AppImage));
    assert_eq!(m.notice(), None, "a later tick brought the notice back");
    assert_eq!(
        m.start_download(),
        None,
        "a dismissed offer cannot be installed"
    );

    let mut m = Model::new(InstallKind::NotOwned(Reason::SystemPackage));
    m.checked(found("1.2.0", Format::AppImage));
    m.later();
    m.checked(found("1.2.0", Format::AppImage));
    assert_eq!(m.notice(), None, "the how-to came back");

    let mut m = Model::new(appimage());
    m.checked(found("1.2.0", Format::AppImage));
    assert!(m.start_download().is_some(), "an offered release downloads");
    m.downloaded(None);
    assert_eq!(m.notice(), Some(Notice::DownloadFailed("1.2.0")));
    m.checked(found("1.3.0", Format::AppImage));
    assert_eq!(
        m.notice(),
        Some(Notice::DownloadFailed("1.2.0")),
        "a tick hid the failure"
    );
    m.later();
    m.checked(found("1.3.0", Format::AppImage));
    assert_eq!(m.notice(), None, "the failure came back");
}

#[test]
fn a_failed_or_empty_check_changes_nothing() {
    let mut m = Model::new(appimage());
    m.checked(Err(CheckError::Offline("connection refused".into())));
    m.checked(Err(CheckError::Http(503)));
    m.checked(Ok(None));
    assert_eq!(m.notice(), None);
    assert!(m.should_check(), "the next tick retries");
    m.checked(found("1.2.0", Format::AppImage));
    m.checked(Err(CheckError::Timeout));
    m.checked(Ok(None));
    assert_eq!(
        m.notice(),
        Some(Notice::Offered("1.2.0")),
        "a quiet tick hid the offer"
    );
    assert_eq!(m.start_download(), Some(&"1.2.0"));
}

#[test]
fn a_newer_tick_replaces_the_offer() {
    let mut m = Model::new(appimage());
    m.checked(found("1.2.0", Format::AppImage));
    assert!(m.should_check(), "an open offer keeps checking");
    m.checked(found("1.3.0", Format::AppImage));
    assert_eq!(m.notice(), Some(Notice::Offered("1.3.0")));
    assert_eq!(
        m.start_download(),
        Some(&"1.3.0"),
        "Install downloads the offer it shows"
    );
}

#[test]
fn a_tick_while_downloading_changes_nothing() {
    let mut m = Model::new(appimage());
    m.checked(found("1.2.0", Format::AppImage));
    assert_eq!(m.start_download(), Some(&"1.2.0"));
    assert!(!m.should_check(), "no check while downloading");
    m.checked(found("1.3.0", Format::AppImage));
    assert_eq!(m.notice(), Some(Notice::Downloading("1.2.0")));
    m.downloaded(Some("staged-1.2.0"));
    assert_eq!(
        m.notice(),
        Some(Notice::Ready("1.2.0")),
        "the 1.2.0 download landed"
    );
}

#[test]
fn ready_skips_further_checks() {
    let mut m = Model::new(appimage());
    m.checked(found("1.2.0", Format::AppImage));
    assert!(m.start_download().is_some(), "an offered release downloads");
    m.downloaded(Some("staged-1.2.0"));
    assert_eq!(m.notice(), Some(Notice::Ready("1.2.0")));
    assert!(!m.should_check(), "no check while an install is armed");
    m.checked(found("1.3.0", Format::AppImage));
    assert_eq!(m.notice(), Some(Notice::Ready("1.2.0")));
    m.later();
    assert_eq!(m.notice(), None);
    let armed = m.take_armed().expect("Later keeps the install armed");
    assert_eq!(armed.staged, "staged-1.2.0");
    assert!(!armed.restart, "Later is not Restart now");
}

#[test]
fn restart_now_arms_the_relaunch_only_when_ready() {
    let mut m = Model::new(appimage());
    assert!(!m.restart_now(), "nothing offered");
    m.checked(found("1.2.0", Format::AppImage));
    assert!(!m.restart_now(), "nothing staged");
    assert!(m.start_download().is_some(), "an offered release downloads");
    m.downloaded(Some("staged-1.2.0"));
    assert!(m.restart_now(), "a staged update restarts");
    assert!(
        m.take_armed().expect("armed").restart,
        "Restart now was pressed"
    );
    assert!(m.take_armed().is_none(), "an install is taken once");
}
