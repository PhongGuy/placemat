//! `check` over a loopback feed: the offer, the header set, signatures, caps and transport.

use crate::support::{Reply, SHA, Server, TestKey, dead_port, feed_config, manifest, publish};
use placemat_core::update::{CheckError, Timeouts, check};
use std::time::Duration;

const ART: &str = "https://example.com/Placemat.AppImage";
const HEADERS: [&str; 4] = ["accept", "accept-encoding", "host", "user-agent"];

fn feed() -> (Server, TestKey) {
    (Server::start(), TestKey::generate())
}

fn own(version: &str) -> String {
    manifest(version, "appimage", ART, SHA)
}

#[test]
fn check_offers_a_newer_signed_release() {
    let (s, k) = feed();
    publish(&s, &k, &own("2.0.0"));
    let a = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"))
        .unwrap()
        .expect("offered");
    assert_eq!((a.version(), a.size(), a.url()), ("2.0.0", 3, ART));
}

#[test]
fn check_ignores_equal_or_older() {
    for old in ["1.0.0", "0.9.0"] {
        let (s, k) = feed();
        publish(&s, &k, &own(old));
        let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
        assert_eq!(got, Ok(None), "{old}");
    }
}

#[test]
fn check_sends_only_the_header_set() {
    let (s, k) = feed();
    publish(&s, &k, &own("2.0.0"));
    check(&feed_config(&s.url("/latest.json"), &k, "1.0.0")).unwrap();
    let seen = s.seen();
    let paths: Vec<&str> = seen.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, ["/latest.json", "/latest.json.sig"]);
    for r in &seen {
        assert_eq!(r.names(), HEADERS, "{}", r.path);
        let ua = concat!("placemat/", env!("CARGO_PKG_VERSION"));
        assert_eq!(r.header("user-agent"), Some(ua));
        assert_eq!(r.header("accept"), Some("*/*"));
        assert_eq!(r.header("accept-encoding"), Some("identity"));
        assert_eq!(r.query, None, "{}", r.path);
    }
}

#[test]
fn check_refuses_bad_signature() {
    let (s, k) = feed();
    let signed = own("2.0.0");
    let edited = signed.replace("2.0.0", "3.0.0");
    s.route("/latest.json", Reply::Body(edited.into_bytes()));
    s.route(
        "/latest.json.sig",
        Reply::Body(k.sign(signed.as_bytes()).into_bytes()),
    );
    let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
    assert!(
        matches!(got, Err(CheckError::ManifestSignature(_))),
        "{got:?}"
    );
}

#[test]
fn check_refuses_unknown_key() {
    let (s, signer) = feed();
    publish(&s, &signer, &own("2.0.0"));
    let trusted = TestKey::generate();
    let got = check(&feed_config(&s.url("/latest.json"), &trusted, "1.0.0"));
    assert_eq!(got, Err(CheckError::UnknownKey));
}

#[test]
fn check_never_parses_unverified_manifest() {
    let (s, k) = feed();
    s.route("/latest.json", Reply::Body(b"this is not json".to_vec()));
    s.route(
        "/latest.json.sig",
        Reply::Body(k.sign(b"something else").into_bytes()),
    );
    let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
    assert!(
        matches!(got, Err(CheckError::ManifestSignature(_))),
        "a parse error would mean parsing came first: {got:?}"
    );
}

#[test]
fn check_signature_http_error_is_signature_error() {
    for status in [404, 500] {
        let (s, k) = feed();
        s.route("/latest.json", Reply::Body(own("2.0.0").into_bytes()));
        s.route("/latest.json.sig", Reply::Status(status));
        let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
        assert!(
            matches!(got, Err(CheckError::ManifestSignature(_))),
            "{status}: {got:?}"
        );
    }
}

#[test]
fn check_manifest_http_error_is_http() {
    let (s, k) = feed();
    let cfg = feed_config(&s.url("/latest.json"), &k, "1.0.0");
    assert_eq!(check(&cfg), Err(CheckError::Http(404)));
    s.route("/latest.json", Reply::Status(500));
    assert_eq!(check(&cfg), Err(CheckError::Http(500)));
}

#[test]
fn check_refuses_oversize_manifest() {
    let (s, k) = feed();
    s.route("/latest.json", Reply::Body(vec![b' '; 64 * 1024 + 1]));
    let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
    assert!(
        matches!(got, Err(CheckError::ManifestInvalid(_))),
        "{got:?}"
    );
    assert_eq!(
        s.seen().len(),
        1,
        "the signature was fetched after an oversize manifest"
    );
}

#[test]
fn check_refuses_oversize_signature() {
    let (s, k) = feed();
    s.route("/latest.json", Reply::Body(own("2.0.0").into_bytes()));
    s.route("/latest.json.sig", Reply::Body(vec![b'A'; 4 * 1024 + 1]));
    let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
    assert!(
        matches!(got, Err(CheckError::ManifestInvalid(_))),
        "{got:?}"
    );
}

// JSON and the signature file both allow trailing whitespace, so each can be padded to its cap.
#[test]
fn check_accepts_bodies_at_their_caps() {
    let (s, k) = feed();
    let mut m = own("2.0.0");
    m.push_str(&" ".repeat(64 * 1024 - m.len()));
    let mut sig = k.sign(m.as_bytes());
    sig.push_str(&"\n".repeat(4 * 1024 - sig.len()));
    s.route("/latest.json", Reply::Body(m.into_bytes()));
    s.route("/latest.json.sig", Reply::Body(sig.into_bytes()));
    let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
    assert!(matches!(got, Ok(Some(_))), "{got:?}");
}

#[test]
fn check_unreachable_is_offline() {
    let k = TestKey::generate();
    let url = format!("http://127.0.0.1:{}/latest.json", dead_port());
    let got = check(&feed_config(&url, &k, "1.0.0"));
    assert!(matches!(got, Err(CheckError::Offline(_))), "{got:?}");
}

#[test]
fn check_dropped_connection_is_offline() {
    let (s, k) = feed();
    s.route("/latest.json", Reply::Hangup);
    let got = check(&feed_config(&s.url("/latest.json"), &k, "1.0.0"));
    assert!(matches!(got, Err(CheckError::Offline(_))), "{got:?}");
}

#[test]
fn check_slow_manifest_times_out() {
    let (s, k) = feed();
    s.route(
        "/latest.json",
        Reply::Slow(Duration::from_secs(5), b"{}".to_vec()),
    );
    let mut short = Timeouts::default();
    short.manifest = Duration::from_millis(200);
    let cfg = feed_config(&s.url("/latest.json"), &k, "1.0.0").with_timeouts(short);
    assert_eq!(check(&cfg), Err(CheckError::Timeout));
}

#[test]
fn check_fetches_the_signature_beside_the_configured_url() {
    let (s, k) = feed();
    let m = own("2.0.0");
    s.route("/a/latest.json", Reply::Redirect(s.url("/b/latest.json")));
    s.route("/b/latest.json", Reply::Body(m.clone().into_bytes()));
    s.route(
        "/a/latest.json.sig",
        Reply::Body(k.sign(m.as_bytes()).into_bytes()),
    );
    s.route("/b/latest.json.sig", Reply::Status(500));
    let got = check(&feed_config(&s.url("/a/latest.json"), &k, "1.0.0"));
    assert!(matches!(got, Ok(Some(_))), "{got:?}");
    let paths: Vec<String> = s.seen().into_iter().map(|r| r.path).collect();
    assert_eq!(
        paths,
        ["/a/latest.json", "/b/latest.json", "/a/latest.json.sig"]
    );
}

#[test]
fn redirect_sends_no_referer() {
    let (s, k) = feed();
    publish(&s, &k, &own("2.0.0"));
    // Two hops for each file, as GitHub's /latest/download/ answers.
    for file in ["latest.json", "latest.json.sig"] {
        s.route(
            &format!("/a/{file}"),
            Reply::Redirect(s.url(&format!("/b/{file}"))),
        );
        s.route(
            &format!("/b/{file}"),
            Reply::Redirect(s.url(&format!("/{file}"))),
        );
    }
    let got = check(&feed_config(&s.url("/a/latest.json"), &k, "1.0.0"));
    assert!(matches!(got, Ok(Some(_))), "{got:?}");
    let seen = s.seen();
    assert_eq!(seen.len(), 6, "two hops and a body for each file");
    for r in &seen {
        assert_eq!(r.header("referer"), None, "{} sent a referer", r.path);
        assert_eq!(r.names(), HEADERS, "{}", r.path);
    }
}

#[test]
fn check_follows_five_redirects() {
    let (s, k) = feed();
    let m = own("2.0.0");
    publish(&s, &k, &m);
    for i in 0..5 {
        let next = match i {
            4 => s.url("/latest.json"),
            _ => s.url(&format!("/hop{}/latest.json", i + 1)),
        };
        s.route(&format!("/hop{i}/latest.json"), Reply::Redirect(next));
    }
    s.route(
        "/hop0/latest.json.sig",
        Reply::Body(k.sign(m.as_bytes()).into_bytes()),
    );
    let got = check(&feed_config(&s.url("/hop0/latest.json"), &k, "1.0.0"));
    assert!(matches!(got, Ok(Some(_))), "{got:?}");
}

#[test]
fn check_refuses_a_sixth_redirect() {
    let (s, k) = feed();
    for i in 0..6 {
        let next = s.url(&format!("/hop{}/latest.json", i + 1));
        s.route(&format!("/hop{i}/latest.json"), Reply::Redirect(next));
    }
    let got = check(&feed_config(&s.url("/hop0/latest.json"), &k, "1.0.0"));
    assert!(matches!(got, Err(CheckError::Offline(_))), "{got:?}");
}
