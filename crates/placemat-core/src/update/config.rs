//! Where a build looks for updates: the release gate, the developer overrides and the URL rule.

use url::{Host, Url};

use super::{
    COMPILED_KEYS, CheckError, DOWNLOAD_PAGE, MANIFEST_URL, Os, Timeouts, UpdateConfig,
    trusted_keys,
};

// A `const fn` cannot compare `str`s, so the flag is matched on its bytes.
const fn is_one(value: Option<&str>) -> bool {
    match value {
        Some(v) => matches!(v.as_bytes(), [b'1']),
        None => false,
    }
}

/// Whether the release workflow built this binary: `PLACEMAT_RELEASE_BUILD` was exactly `1`
/// at compile time, the same test the build script applies.
pub const RELEASE_BUILD: bool = is_one(option_env!("PLACEMAT_RELEASE_BUILD"));

/// The manifest target id for this host, `app-<os>-<arch>`, or `None` on a system or
/// architecture no release ships for.
pub fn target_id() -> Option<String> {
    let os = match Os::current()? {
        Os::Linux => "linux",
        Os::Macos => "macos",
        Os::Windows => "windows",
    };
    let arch = match std::env::consts::ARCH {
        arch @ ("x86_64" | "aarch64") => arch,
        _ => return None,
    };
    Some(format!("app-{os}-{arch}"))
}

/// The update configuration from explicit inputs. An opt-out of exactly `"1"`, an unknown
/// host, or no usable key is [`CheckError::Disabled`]. A release build ignores both overrides
/// and uses [`MANIFEST_URL`] with `compiled_keys`; any other build is disabled unless it has a
/// manifest override, and then trusts the keys override or the compiled keys and allows plain
/// http to loopback. A manifest URL outside the URL rule is [`CheckError::ManifestInvalid`].
/// Empty override strings count as absent.
pub fn build_config_with(
    release_build: bool,
    opt_out: Option<&str>,
    manifest_override: Option<&str>,
    keys_override: Option<&str>,
    compiled_keys: &str,
    current_version: &str,
    target_id: Option<&str>,
) -> Result<UpdateConfig, CheckError> {
    if opt_out == Some("1") {
        return Err(CheckError::Disabled);
    }
    let Some(target_id) = target_id else {
        return Err(CheckError::Disabled);
    };
    let manifest_override = manifest_override.filter(|u| !u.is_empty());
    let keys_override = keys_override.filter(|k| !k.is_empty());
    // Only a source build may point at another feed, so a release binary trusts nothing but
    // the public feed and the keys compiled into it.
    let (manifest_url, keys_text, allow_loopback_http) = if release_build {
        (MANIFEST_URL, compiled_keys, false)
    } else {
        let Some(url) = manifest_override else {
            return Err(CheckError::Disabled);
        };
        (url, keys_override.unwrap_or(compiled_keys), true)
    };
    let keys = trusted_keys(keys_text).map_err(|_| CheckError::Disabled)?;
    if !url_allowed(manifest_url, allow_loopback_http) {
        return Err(CheckError::ManifestInvalid(format!(
            "the manifest URL must be https: {manifest_url:?}"
        )));
    }
    Ok(UpdateConfig {
        manifest_url: manifest_url.to_owned(),
        download_page: DOWNLOAD_PAGE.to_owned(),
        keys,
        current_version: current_version.to_owned(),
        target_id: target_id.to_owned(),
        timeouts: Timeouts::default(),
        allow_loopback_http,
    })
}

/// The configuration of this build. `PLACEMAT_NO_UPDATE_CHECK=1` opts out; a non-release
/// build also reads `PLACEMAT_UPDATE_MANIFEST_URL` and `PLACEMAT_TEST_UPDATE_KEYS`, which a
/// release build never looks at. See [`build_config_with`] for the rules.
pub fn build_config() -> Result<UpdateConfig, CheckError> {
    let var = |name: &str| std::env::var(name).ok();
    let opt_out = var("PLACEMAT_NO_UPDATE_CHECK");
    let (manifest_override, keys_override) = if RELEASE_BUILD {
        (None, None)
    } else {
        (
            var("PLACEMAT_UPDATE_MANIFEST_URL"),
            var("PLACEMAT_TEST_UPDATE_KEYS"),
        )
    };
    build_config_with(
        RELEASE_BUILD,
        opt_out.as_deref(),
        manifest_override.as_deref(),
        keys_override.as_deref(),
        COMPILED_KEYS,
        env!("CARGO_PKG_VERSION"),
        target_id().as_deref(),
    )
}

/// The URL rule for update traffic: `https` always, plain `http` only to a loopback host and
/// only when `allow_loopback_http` is set. Any other scheme, or an unparsable URL, is refused.
pub(crate) fn url_allowed(url: &str, allow_loopback_http: bool) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" if allow_loopback_http => match url.host() {
            Some(Host::Domain(name)) => name == "localhost",
            Some(Host::Ipv4(ip)) => ip.is_loopback(),
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_one, url_allowed};

    #[test]
    fn loopback_http_needs_the_allowance() {
        assert!(!url_allowed("http://127.0.0.1:9/x", false));
    }

    #[test]
    fn the_release_flag_is_exactly_one() {
        assert!(is_one(Some("1")));
        for v in [
            None,
            Some(""),
            Some("0"),
            Some("11"),
            Some("true"),
            Some(" 1"),
        ] {
            assert!(!is_one(v), "{v:?}");
        }
    }
}
