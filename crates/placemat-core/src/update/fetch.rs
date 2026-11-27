//! The update check: the updater's agent sends only a fixed header set, never decodes bodies,
//! and caps every body on the wire. Every call blocks.

use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig};
use ureq::{Agent, Body, http::Response};

use super::{Available, CheckError, Timeouts, UpdateConfig, evaluate, verify_manifest};

const MANIFEST_CAP: u64 = 64 * 1024;
const SIGNATURE_CAP: u64 = 4 * 1024;
const MAX_REDIRECTS: u32 = 5;

/// Fetches the manifest (at most 64 KiB) and the manifest URL plus `.sig` (at most 4 KiB),
/// verifies the signature, then evaluates. `Ok(None)` when no newer stable release exists.
/// A non-success manifest status is [`CheckError::Http`], a non-success signature status is
/// [`CheckError::ManifestSignature`], a body over its cap is [`CheckError::ManifestInvalid`],
/// a time limit is [`CheckError::Timeout`] and any other transport failure is
/// [`CheckError::Offline`].
pub fn check(cfg: &UpdateConfig) -> Result<Option<Available>, CheckError> {
    // Plain http is allowed only by the config bit, never by what the URL looks like.
    let https_only = !cfg.allow_loopback_http;
    let t = &cfg.timeouts;

    let response = get(&agent(t, t.manifest, https_only), &cfg.manifest_url)?;
    if !response.status().is_success() {
        return Err(CheckError::Http(response.status().as_u16()));
    }
    let manifest = read_capped(response, MANIFEST_CAP, "the manifest")?;

    // The signature sits beside the configured URL, wherever the manifest was redirected to.
    let sig_url = format!("{}.sig", cfg.manifest_url);
    let response = get(&agent(t, t.manifest, https_only), &sig_url)?;
    if !response.status().is_success() {
        return Err(CheckError::ManifestSignature(format!(
            "the signature answered HTTP {}",
            response.status().as_u16()
        )));
    }
    let signature = read_capped(response, SIGNATURE_CAP, "the signature")?;

    // The bytes verified are the bytes parsed; nothing is read from an unverified manifest.
    verify_manifest(&manifest, &signature, &cfg.keys)?;
    evaluate(&manifest, cfg)
}

/// The updater's agent: a fixed header set, at most five redirects, platform roots, and
/// `total` as the whole request's time limit. Without `https_only` the environment proxy is
/// dropped too, so loopback traffic never leaves the machine.
pub(crate) fn agent(timeouts: &Timeouts, total: Duration, https_only: bool) -> Agent {
    let mut config = Agent::config_builder()
        .http_status_as_error(false)
        .https_only(https_only)
        .max_redirects(MAX_REDIRECTS)
        .user_agent(concat!("placemat/", env!("CARGO_PKG_VERSION")))
        .accept("*/*")
        .accept_encoding("identity")
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(timeouts.connect))
        .timeout_global(Some(total));
    if !https_only {
        config = config.proxy(None);
    }
    Agent::new_with_config(config.build())
}

fn get(agent: &Agent, url: &str) -> Result<Response<Body>, CheckError> {
    agent.get(url).call().map_err(transport)
}

/// Maps a request or body error: a time limit is `Timeout`, anything else (a refused http
/// hop or too many redirects included) means the server could not be reached.
pub(crate) fn transport(e: ureq::Error) -> CheckError {
    if matches!(e, ureq::Error::Timeout(_)) {
        CheckError::Timeout
    } else {
        CheckError::Offline(e.to_string())
    }
}

/// The whole body, refused as soon as it holds more than `cap` bytes.
fn read_capped(response: Response<Body>, cap: u64, what: &str) -> Result<Vec<u8>, CheckError> {
    response
        .into_body()
        .into_with_config()
        .limit(cap + 1)
        .read_to_vec()
        .map_err(|e| {
            // `matches!`, not a binding arm: the workspace warns on wildcard enum arms.
            if matches!(e, ureq::Error::BodyExceedsLimit(_)) {
                CheckError::ManifestInvalid(format!("{what} is larger than {cap} bytes"))
            } else {
                transport(e)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::{agent, check, transport};
    use crate::update::{CheckError, Timeouts, UpdateConfig};
    use std::net::TcpListener;
    use std::time::Duration;

    // Short limits, so a regression that lets the request through fails fast instead of hanging.
    fn short() -> Timeouts {
        let s = Duration::from_secs(2);
        Timeouts {
            connect: s,
            manifest: s,
            artifact: s,
        }
    }

    // A listener that is never accepted from: a refused request must leave its backlog empty.
    fn silent() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/latest.json", listener.local_addr().unwrap());
        (listener, url)
    }

    fn nothing_arrived(listener: &TcpListener) -> bool {
        matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
    }

    #[test]
    fn agent_https_only_refuses_http_hop() {
        let (listener, url) = silent();
        let t = short();
        let err = agent(&t, t.manifest, true).get(&url).call().unwrap_err();
        assert!(matches!(err, ureq::Error::RequireHttpsOnly(_)), "{err:?}");
        assert!(matches!(transport(err), CheckError::Offline(_)));
        assert!(nothing_arrived(&listener), "the request reached the server");
    }

    // A release config can never hold this URL; built by hand, it proves the agent's https rule
    // follows the config bit rather than the URL.
    #[test]
    fn check_without_loopback_allowance_refuses_http() {
        let (listener, url) = silent();
        let cfg = UpdateConfig {
            manifest_url: url,
            download_page: String::new(),
            keys: Vec::new(),
            current_version: "1.0.0".into(),
            target_id: "app-linux-x86_64".into(),
            timeouts: short(),
            allow_loopback_http: false,
        };
        let got = check(&cfg);
        assert!(matches!(got, Err(CheckError::Offline(_))), "{got:?}");
        assert!(nothing_arrived(&listener), "the request reached the server");
    }
}
