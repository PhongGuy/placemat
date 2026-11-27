//! `build_config_with`: the opt-out, the release gate, the overrides and the URL rule.

use crate::support::{KEY_LINE, TARGET};
use placemat_core::update::{
    COMPILED_KEYS, CheckError, DOWNLOAD_PAGE, MANIFEST_URL, Os, PLACEHOLDER, Timeouts,
    UpdateConfig, build_config_with, target_id, trusted_keys,
};
use std::time::Duration;

const LOCAL: &str = "http://127.0.0.1:9/latest.json";

// A release build whose compiled keys are KEY_LINE.
fn release(
    opt_out: Option<&str>,
    url: Option<&str>,
    keys: Option<&str>,
) -> Result<UpdateConfig, CheckError> {
    build_config_with(true, opt_out, url, keys, KEY_LINE, "1.0.0", Some(TARGET))
}

// A non-release build whose compiled keys are the committed release keys.
fn source(url: Option<&str>, keys: Option<&str>) -> Result<UpdateConfig, CheckError> {
    build_config_with(false, None, url, keys, COMPILED_KEYS, "1.0.0", Some(TARGET))
}

#[test]
fn config_release_uses_the_feed_and_the_compiled_keys() {
    let cfg = release(None, None, None).unwrap();
    assert_eq!(cfg.manifest_url(), MANIFEST_URL);
    assert_eq!(cfg.download_page(), DOWNLOAD_PAGE);
    assert_eq!(cfg.keys(), [KEY_LINE]);
    assert_eq!(cfg.current_version(), "1.0.0");
    assert_eq!(cfg.target_id(), TARGET);
    assert!(!cfg.allow_loopback_http());
    assert_eq!(cfg.timeouts(), Timeouts::default());
    let t = Timeouts::default();
    assert_eq!(
        (t.connect, t.manifest, t.artifact),
        (
            Duration::from_secs(10),
            Duration::from_secs(30),
            Duration::from_secs(15 * 60)
        )
    );
    assert_eq!(
        DOWNLOAD_PAGE,
        "https://github.com/PhongGuy/placemat/releases/latest"
    );
}

#[test]
fn config_opt_out_disables() {
    for release_build in [true, false] {
        let got = build_config_with(
            release_build,
            Some("1"),
            Some(LOCAL),
            Some(KEY_LINE),
            KEY_LINE,
            "1.0.0",
            Some(TARGET),
        );
        assert_eq!(
            got.err(),
            Some(CheckError::Disabled),
            "release {release_build}"
        );
    }
    for other in [None, Some(""), Some("0"), Some("true"), Some(" 1")] {
        assert!(release(other, None, None).is_ok(), "{other:?}");
    }
}

#[test]
fn config_release_ignores_overrides() {
    let cfg = release(None, Some(LOCAL), Some(COMPILED_KEYS)).unwrap();
    assert_eq!(cfg.manifest_url(), MANIFEST_URL);
    assert_eq!(cfg.keys(), [KEY_LINE]);
    assert!(!cfg.allow_loopback_http());
}

#[test]
fn config_release_never_allows_loopback_http() {
    let cases = [
        (None, None),
        (Some(LOCAL), None),
        (Some("https://example.com/latest.json"), Some(KEY_LINE)),
        (Some(""), Some("")),
    ];
    for (url, keys) in cases {
        let cfg = release(None, url, keys).unwrap();
        assert!(!cfg.allow_loopback_http(), "{url:?} {keys:?}");
    }
}

#[test]
fn config_release_without_a_usable_key_is_disabled() {
    for compiled in ["", "# no key yet\n", PLACEHOLDER, "not-a-key"] {
        let got = build_config_with(true, None, None, None, compiled, "1.0.0", Some(TARGET));
        assert_eq!(got.err(), Some(CheckError::Disabled), "{compiled:?}");
    }
}

#[test]
fn config_nonrelease_without_override_is_disabled() {
    for url in [None, Some("")] {
        assert_eq!(
            source(url, Some(KEY_LINE)).err(),
            Some(CheckError::Disabled),
            "{url:?}"
        );
    }
}

#[test]
fn config_nonrelease_override_takes_the_test_keys() {
    let cfg = source(Some(LOCAL), Some(KEY_LINE)).unwrap();
    assert_eq!(cfg.manifest_url(), LOCAL);
    assert_eq!(cfg.keys(), [KEY_LINE]);
    assert!(cfg.allow_loopback_http());
    let compiled = trusted_keys(COMPILED_KEYS).unwrap();
    for keys in [None, Some("")] {
        let cfg = source(Some(LOCAL), keys).unwrap();
        assert_eq!(cfg.keys(), compiled.as_slice(), "{keys:?}");
    }
    assert_eq!(
        source(Some(LOCAL), Some("not-a-key")).err(),
        Some(CheckError::Disabled)
    );
}

#[test]
fn config_refuses_non_loopback_http() {
    let refused = [
        "http://example.com/latest.json",
        "http://10.0.0.1/latest.json",
        "http://127.0.0.1.example.com/latest.json",
        "ftp://127.0.0.1/latest.json",
        "file:///tmp/latest.json",
        "not a url",
    ];
    for url in refused {
        let got = source(Some(url), Some(KEY_LINE));
        assert!(
            matches!(got, Err(CheckError::ManifestInvalid(_))),
            "{url}: {got:?}"
        );
    }
    let allowed = [
        "http://localhost:9/latest.json",
        "http://127.0.0.2:9/latest.json",
        "http://[::1]:9/latest.json",
        "https://example.com/latest.json",
    ];
    for url in allowed {
        assert!(source(Some(url), Some(KEY_LINE)).is_ok(), "{url}");
    }
}

#[test]
fn config_unknown_host_is_disabled() {
    let release = build_config_with(true, None, None, None, KEY_LINE, "1.0.0", None);
    let local = build_config_with(false, None, Some(LOCAL), Some(KEY_LINE), "", "1.0.0", None);
    assert_eq!(release.err(), Some(CheckError::Disabled));
    assert_eq!(local.err(), Some(CheckError::Disabled));
}

#[test]
fn target_id_names_this_host() {
    use std::env::consts::{ARCH, OS};
    let expected = match (OS, ARCH) {
        ("linux" | "macos" | "windows", "x86_64" | "aarch64") => Some(format!("app-{OS}-{ARCH}")),
        _ => None,
    };
    assert_eq!(target_id(), expected);
}

#[test]
fn os_current_follows_the_host() {
    let expected = match std::env::consts::OS {
        "linux" => Some(Os::Linux),
        "macos" => Some(Os::Macos),
        "windows" => Some(Os::Windows),
        _ => None,
    };
    assert_eq!(Os::current(), expected);
}
