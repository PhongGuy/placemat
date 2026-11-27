//! `evaluate`: the manifest rules on bytes whose signature already verified.

use crate::support::{KEY_LINE, SHA, TARGET, manifest, signed_evaluate};
use placemat_core::update::{CheckError, Format, UpdateConfig, build_config_with};

const ART: &str = "https://example.com/Placemat.AppImage";

// A release-build config, so loopback http is not allowed.
fn release(current: &str) -> UpdateConfig {
    build_config_with(true, None, None, None, KEY_LINE, current, Some(TARGET)).unwrap()
}

// A non-release config under the test feed override, so loopback http is allowed.
fn local(current: &str) -> UpdateConfig {
    let url = Some("http://127.0.0.1:9/latest.json");
    build_config_with(false, None, url, Some(KEY_LINE), "", current, Some(TARGET)).unwrap()
}

fn own(version: &str) -> String {
    manifest(version, "appimage", ART, SHA)
}

#[test]
fn evaluate_offers_a_newer_stable_release() {
    let a = signed_evaluate(own("1.2.0").as_bytes(), &release("1.1.9"))
        .unwrap()
        .expect("offered");
    assert_eq!(a.version(), "1.2.0");
    assert_eq!(a.pub_date(), Some("2026-11-01T12:00:00Z"));
    assert_eq!(a.format(), Format::AppImage);
    assert_eq!((a.url(), a.sha256(), a.size()), (ART, SHA, 3));
}

#[test]
fn evaluate_offers_only_strictly_newer() {
    for (remote, current) in [
        ("1.0.1", "1.0.0"),
        ("2.0.0", "1.9.9"),
        ("1.0.0", "1.0.0-dev"),
    ] {
        let got = signed_evaluate(own(remote).as_bytes(), &release(current)).unwrap();
        assert!(got.is_some(), "{remote} against {current}");
    }
}

#[test]
fn evaluate_ignores_equal() {
    for (remote, current) in [("1.0.0", "1.0.0"), ("1.0.0+rebuild", "1.0.0")] {
        assert_eq!(
            signed_evaluate(own(remote).as_bytes(), &release(current)),
            Ok(None),
            "{remote}"
        );
    }
}

#[test]
fn evaluate_ignores_older() {
    for remote in ["0.9.9", "0.1.0"] {
        assert_eq!(
            signed_evaluate(own(remote).as_bytes(), &release("1.0.0")),
            Ok(None),
            "{remote}"
        );
    }
}

#[test]
fn evaluate_refuses_prerelease() {
    for v in ["1.2.0-rc.1", "9.0.0-beta"] {
        let got = signed_evaluate(own(v).as_bytes(), &release("0.1.0"));
        assert!(
            matches!(got, Err(CheckError::ManifestInvalid(_))),
            "{v}: {got:?}"
        );
    }
}

#[test]
fn evaluate_refuses_bad_version() {
    let remote = signed_evaluate(own("two").as_bytes(), &release("1.0.0"));
    assert!(
        matches!(remote, Err(CheckError::ManifestInvalid(_))),
        "{remote:?}"
    );
    let current = signed_evaluate(own("2.0.0").as_bytes(), &release("not-semver"));
    assert!(
        matches!(current, Err(CheckError::ManifestInvalid(_))),
        "{current:?}"
    );
}

#[test]
fn evaluate_missing_target_is_no_entry() {
    let m = own("2.0.0").replace(TARGET, "app-plan9-mips");
    assert_eq!(
        signed_evaluate(m.as_bytes(), &release("0.1.0")),
        Err(CheckError::NoEntryForTarget(TARGET.to_owned()))
    );
}

#[test]
fn evaluate_refuses_malformed_documents_and_entries() {
    let bad = [
        "not json".to_owned(),
        r#"{"targets":{}}"#.to_owned(),
        format!(
            r#"{{"version":"2.0.0","targets":{{"{TARGET}":{{"url":"{ART}","format":"appimage","sha256":"{SHA}"}}}}}}"#
        ),
        manifest("2.0.0", "dmg", ART, SHA),
        manifest("2.0.0", "binary", ART, SHA),
        manifest("2.0.0", "appimage", "http://example.com/a", SHA),
        manifest("2.0.0", "appimage", "ftp://example.com/a", SHA),
        manifest("2.0.0", "appimage", ART, "abc"),
        manifest("2.0.0", "appimage", ART, &SHA.to_uppercase()),
    ];
    for m in bad {
        let got = signed_evaluate(m.as_bytes(), &release("0.1.0"));
        assert!(
            matches!(got, Err(CheckError::ManifestInvalid(_))),
            "{m}: {got:?}"
        );
    }
}

#[test]
fn evaluate_maps_every_format() {
    for (name, format) in [
        ("appimage", Format::AppImage),
        ("app", Format::App),
        ("nsis", Format::Nsis),
    ] {
        let m = manifest("2.0.0", name, ART, SHA);
        let a = signed_evaluate(m.as_bytes(), &release("0.1.0"))
            .unwrap()
            .expect("offered");
        assert_eq!(a.format(), format, "{name}");
    }
}

#[test]
fn evaluate_refuses_loopback_http_artifact_when_not_allowed() {
    let m = manifest("2.0.0", "appimage", "http://127.0.0.1:9/a", SHA);
    let got = signed_evaluate(m.as_bytes(), &release("0.1.0"));
    assert!(
        matches!(got, Err(CheckError::ManifestInvalid(_))),
        "{got:?}"
    );
    let a = signed_evaluate(m.as_bytes(), &local("0.1.0"))
        .unwrap()
        .expect("offered under the override");
    assert_eq!(a.url(), "http://127.0.0.1:9/a");
    let far = manifest("2.0.0", "appimage", "http://example.com/a", SHA);
    let got = signed_evaluate(far.as_bytes(), &local("0.1.0"));
    assert!(
        matches!(got, Err(CheckError::ManifestInvalid(_))),
        "{got:?}"
    );
}
